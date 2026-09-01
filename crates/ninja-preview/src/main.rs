//! ninja-preview：单文件编辑器插件，独立进程。
//!
//! ⌘+click 路径 → 新标签打开该文件，可编辑、⌘S 保存。官方不特权——
//! 与社区插件走同一套 ADE 协议，只经 Unix socket 交换 JSON 帧，
//! 永不链宿主内部 API（`cargo tree -p ninja-preview` 无宿主 crate）。
//!
//! ```text
//! 宿主 spawn（env NINJA_ADE_SOCK）→ connect
//! ← hit
//! → hit.claim / hit.ignore
//! → layer.open(tab, surface=html)     （同路径已开则 layer.msg goto）
//! ← layer.ready
//! → layer.html                        （编辑器壳 + 源码）
//! ↔ layer.msg  name=draft|save|goto|saved
//! ← layer.close                       （dirty 则把最后一份 draft 写盘）
//! ```
//!
//! 读盘/写盘是插件本地行为，不经协议。JS 不能指定路径。

use std::collections::HashMap;
use std::io::{Read, Write};
use std::os::unix::net::UnixStream;
use std::path::PathBuf;

use ninja_protocol::frame::{encode_frame, FrameDecoder};
use ninja_protocol::{
    DecodeError, Hit, HitClaim, HitIgnore, HitKind, LayerClose, LayerHtml, LayerMsg, LayerOpen,
    LayerReady, Message, Placement, Surface, MAX_FRAME_BYTES, PROTOCOL_VERSION,
};

mod editor;

const SOCK_ENV: &str = "NINJA_ADE_SOCK";
const CLAIM_PRIORITY: u32 = 100;
const MAX_FILE_BYTES: u64 = 8 * 1024 * 1024;
const SNIFF_BYTES: usize = 8 * 1024;

fn main() {
    let code = run();
    std::process::exit(code);
}

fn run() -> i32 {
    let Some(sock) = std::env::var_os(SOCK_ENV) else {
        eprintln!("ninja-preview: 缺 {SOCK_ENV}（应由宿主拉起）");
        return 2;
    };
    let mut stream = None;
    for _ in 0..100 {
        match UnixStream::connect(&sock) {
            Ok(s) => {
                stream = Some(s);
                break;
            }
            Err(_) => std::thread::sleep(std::time::Duration::from_millis(50)),
        }
    }
    let Some(mut stream) = stream else {
        eprintln!("ninja-preview: 连不上 ADE socket {sock:?}");
        return 2;
    };
    eprintln!("ninja-preview: 已连接宿主（v0 编辑器）");

    let mut decoder = FrameDecoder::new();
    let mut st = PluginState::default();
    let mut buf = [0u8; 8192];
    loop {
        match stream.read(&mut buf) {
            Ok(0) => {
                flush_all(&mut st);
                return 0;
            }
            Ok(n) => {
                if decoder.extend(&buf[..n]).is_err() {
                    eprintln!("ninja-preview: 帧缓冲超限，断开");
                    flush_all(&mut st);
                    return 2;
                }
            }
            Err(e) if e.kind() == std::io::ErrorKind::Interrupted => continue,
            Err(e) => {
                eprintln!("ninja-preview: socket 读失败：{e}");
                flush_all(&mut st);
                return 2;
            }
        }
        while let Some(payload) = decoder.pop() {
            let payload = match payload {
                Ok(p) => p,
                Err(e) => {
                    eprintln!("ninja-preview: 帧级违规（{e}），断开");
                    flush_all(&mut st);
                    return 2;
                }
            };
            match Message::decode_plugin(&payload) {
                Ok(msg) => {
                    if !handle(&mut st, &msg, &mut stream) {
                        return 0;
                    }
                }
                Err(DecodeError::UnsupportedVersion { got, supported }) => {
                    eprintln!("ninja-preview: 协议版本 v{got} 不支持（本实现 v{supported}），退出");
                    flush_all(&mut st);
                    return 78;
                }
                Err(e) => {
                    eprintln!("ninja-preview: 丢弃无法解码的消息：{e}");
                }
            }
        }
    }
}

#[derive(Default)]
struct PluginState {
    /// layer.open id → 尚未 ready 的目标。
    pending: HashMap<u64, Target>,
    /// layer 句柄 → 打开的文件。
    buffers: HashMap<u64, Buffer>,
}

struct Buffer {
    path: PathBuf,
    content: String,
    dirty: bool,
}

#[derive(Clone)]
pub(crate) struct Target {
    pub path: PathBuf,
    pub line: Option<u32>,
    pub col: Option<u32>,
}

fn handle(st: &mut PluginState, msg: &Message, stream: &mut UnixStream) -> bool {
    match msg {
        Message::Hit(hit) => on_hit(st, hit, stream),
        Message::LayerReady(ready) => on_layer_ready(st, ready, stream),
        Message::LayerMsg(m) => on_layer_msg(st, m, stream),
        Message::LayerClose(close) => {
            close_buffer(st, close.layer);
            true
        }
        _ => true,
    }
}

fn on_hit(st: &mut PluginState, hit: &Hit, stream: &mut UnixStream) -> bool {
    if std::env::var_os("NINJA_ADE_DEBUG").is_some() {
        eprintln!(
            "ninja-preview: hit id={} kind={:?} text={:?} cwd={:?}",
            hit.id, hit.kind, hit.text, hit.cwd
        );
    }
    match claimable_target(hit) {
        Some(target) => {
            eprintln!(
                "ninja-preview: claim {}（{}:{}）",
                target.path.display(),
                target.line.unwrap_or(1),
                target.col.unwrap_or(1)
            );
            let _ = send(
                stream,
                &Message::HitClaim(HitClaim::new(hit.id, CLAIM_PRIORITY)),
            );
            if let Some(layer) = layer_for_path(st, &target.path) {
                let body = format!(
                    "{}:{}",
                    target.line.unwrap_or(1),
                    target.col.unwrap_or(1)
                );
                let _ = send(
                    stream,
                    &Message::LayerMsg(LayerMsg::new(layer, "goto", body)),
                );
                return true;
            }
            let title = target
                .path
                .file_name()
                .and_then(|s| s.to_str())
                .unwrap_or("untitled")
                .to_string();
            let _ = send(
                stream,
                &Message::LayerOpen(
                    LayerOpen::new(hit.id, Placement::Tab, hit.row, hit.col)
                        .with_title(title)
                        .with_surface(Surface::Html),
                ),
            );
            st.pending.insert(hit.id, target);
            true
        }
        None => {
            let _ = send(stream, &Message::HitIgnore(HitIgnore::new(hit.id)));
            true
        }
    }
}

fn on_layer_ready(st: &mut PluginState, ready: &LayerReady, stream: &mut UnixStream) -> bool {
    let Some(target) = st.pending.remove(&ready.id) else {
        let _ = send(stream, &Message::LayerClose(LayerClose::new(ready.layer)));
        return true;
    };
    let content = match std::fs::read_to_string(&target.path) {
        Ok(c) => c,
        Err(e) => {
            eprintln!("ninja-preview: 读 {} 失败：{e}", target.path.display());
            let _ = send(stream, &Message::LayerClose(LayerClose::new(ready.layer)));
            return true;
        }
    };
    let html = editor::document(&target.path, &content, target.line, target.col);
    if html.len() + 256 > MAX_FRAME_BYTES {
        eprintln!(
            "ninja-preview: {} 渲染后超过单帧上限，放弃",
            target.path.display()
        );
        let _ = send(stream, &Message::LayerClose(LayerClose::new(ready.layer)));
        return true;
    }
    st.buffers.insert(
        ready.layer,
        Buffer {
            path: target.path.clone(),
            content,
            dirty: false,
        },
    );
    let _ = send(
        stream,
        &Message::LayerHtml(LayerHtml::new(ready.layer, html)),
    );
    true
}

fn on_layer_msg(st: &mut PluginState, m: &LayerMsg, stream: &mut UnixStream) -> bool {
    match m.name.as_str() {
        "draft" => {
            if let Some(buf) = st.buffers.get_mut(&m.layer)
                && buf.content != m.body
            {
                buf.content = m.body.clone();
                buf.dirty = true;
            }
        }
        "save" => {
            if let Some(buf) = st.buffers.get_mut(&m.layer) {
                buf.content = m.body.clone();
                buf.dirty = true;
                match write_buf(buf) {
                    Ok(()) => {
                        let _ = send(
                            stream,
                            &Message::LayerMsg(LayerMsg::new(m.layer, "saved", "")),
                        );
                    }
                    Err(e) => {
                        eprintln!("ninja-preview: 写 {} 失败：{e}", buf.path.display());
                        let _ = send(
                            stream,
                            &Message::LayerMsg(LayerMsg::new(m.layer, "error", e.to_string())),
                        );
                    }
                }
            }
        }
        _ => {}
    }
    true
}

fn layer_for_path(st: &PluginState, path: &std::path::Path) -> Option<u64> {
    st.buffers
        .iter()
        .find(|(_, b)| b.path == path)
        .map(|(id, _)| *id)
}

fn close_buffer(st: &mut PluginState, layer: u64) {
    if let Some(mut buf) = st.buffers.remove(&layer)
        && buf.dirty
    {
        if let Err(e) = write_buf(&mut buf) {
            eprintln!("ninja-preview: 关层写回 {} 失败：{e}", buf.path.display());
        } else {
            eprintln!("ninja-preview: 关层写回 {}", buf.path.display());
        }
    }
}

fn flush_all(st: &mut PluginState) {
    let layers: Vec<u64> = st.buffers.keys().copied().collect();
    for layer in layers {
        close_buffer(st, layer);
    }
}

fn write_buf(buf: &mut Buffer) -> std::io::Result<()> {
    std::fs::write(&buf.path, buf.content.as_bytes())?;
    buf.dirty = false;
    eprintln!("ninja-preview: 已写 {}", buf.path.display());
    Ok(())
}

fn send(stream: &mut UnixStream, msg: &Message) -> std::io::Result<()> {
    let frame = encode_frame(msg).map_err(|e| std::io::Error::other(format!("encode: {e}")))?;
    stream.write_all(&frame)
}

fn claimable_target(hit: &Hit) -> Option<Target> {
    if hit.v != PROTOCOL_VERSION {
        return None;
    }
    if hit.kind != HitKind::Path {
        return None;
    }
    let (bare, line, col) = strip_line_col(&hit.text);
    if bare.is_empty() {
        return None;
    }
    let bare = file_url_to_fs_path(bare).unwrap_or_else(|| bare.to_string());
    let cwd = file_url_to_fs_path(&hit.cwd).unwrap_or_else(|| hit.cwd.clone());
    let resolved = resolve_path(&bare, &cwd)?;
    let meta = std::fs::metadata(&resolved).ok()?;
    if !meta.is_file() || meta.len() > MAX_FILE_BYTES {
        return None;
    }
    if is_probably_binary(&resolved) {
        return None;
    }
    Some(Target {
        path: resolved,
        line,
        col,
    })
}

fn strip_line_col(s: &str) -> (&str, Option<u32>, Option<u32>) {
    let b = s.as_bytes();
    let mut end = b.len();
    let mut nums: Vec<u32> = Vec::new();
    for _ in 0..2 {
        let Some(colon) = b[..end].iter().rposition(|&c| c == b':') else {
            break;
        };
        let digits = &b[colon + 1..end];
        if digits.is_empty() || !digits.iter().all(u8::is_ascii_digit) {
            break;
        }
        let Ok(n) = std::str::from_utf8(digits).unwrap().parse::<u32>() else {
            break;
        };
        nums.push(n);
        end = colon;
    }
    let bare = &s[..end];
    match nums.len() {
        0 => (bare, None, None),
        1 => (bare, Some(nums[0]), None),
        _ => (bare, Some(nums[1]), Some(nums[0])),
    }
}

fn file_url_to_fs_path(s: &str) -> Option<String> {
    let rest = s.strip_prefix("file://")?;
    let path = if rest.starts_with('/') {
        rest
    } else {
        let slash = rest.find('/')?;
        &rest[slash..]
    };
    Some(path.to_string())
}

fn resolve_path(bare: &str, cwd: &str) -> Option<PathBuf> {
    let home = std::env::var_os("HOME");
    resolve_path_with(bare, cwd, home.as_deref())
}

fn resolve_path_with(bare: &str, cwd: &str, home: Option<&std::ffi::OsStr>) -> Option<PathBuf> {
    if let Some(rest) = bare.strip_prefix("~/") {
        let home = home?;
        return Some(PathBuf::from(home).join(rest));
    }
    if bare.starts_with('/') {
        return Some(PathBuf::from(bare));
    }
    if bare == "~" {
        return home.map(PathBuf::from);
    }
    if bare.starts_with('~') {
        return None;
    }
    if cwd.is_empty() {
        return None;
    }
    Some(PathBuf::from(cwd).join(bare))
}

fn is_probably_binary(path: &std::path::Path) -> bool {
    let mut f = match std::fs::File::open(path) {
        Ok(f) => f,
        Err(_) => return true,
    };
    let mut head = vec![0u8; SNIFF_BYTES.min(4096)];
    let n = f.read(&mut head).unwrap_or(0);
    head[..n].contains(&0)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn hit(text: &str, cwd: &str) -> Hit {
        Hit::new(1, HitKind::Path, text, cwd, 0, 0, 1, vec![])
    }

    #[test]
    fn line_col_stripped() {
        assert_eq!(
            strip_line_col("/a/b/main.rs:42:13"),
            ("/a/b/main.rs", Some(42), Some(13))
        );
        assert_eq!(
            strip_line_col("/a/b/main.rs:42"),
            ("/a/b/main.rs", Some(42), None)
        );
        assert_eq!(strip_line_col("/a/b.rs"), ("/a/b.rs", None, None));
        assert_eq!(strip_line_col("/a/b:c/d.txt").0, "/a/b:c/d.txt");
    }

    #[test]
    fn resolve_rules() {
        let t = Some(std::ffi::OsStr::new("/Users/t"));
        assert_eq!(
            resolve_path_with("/abs/x.rs", "whatever", t).unwrap(),
            PathBuf::from("/abs/x.rs")
        );
        assert_eq!(
            resolve_path_with("~/x/y.rs", "", t).unwrap(),
            PathBuf::from("/Users/t/x/y.rs")
        );
        assert_eq!(
            resolve_path_with("src/main.rs", "/repo", None).unwrap(),
            PathBuf::from("/repo/src/main.rs")
        );
        assert!(resolve_path_with("src/main.rs", "", t).is_none());
        assert_eq!(resolve_path_with("~other/x", "/repo", t), None);
        assert_eq!(resolve_path_with("~", "", None), None);
        assert_eq!(
            resolve_path_with("~", "", t).unwrap(),
            PathBuf::from("/Users/t")
        );
    }

    #[test]
    fn claimable_rules() {
        let dir = std::env::temp_dir().join(format!("ninja_prev_{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let txt = dir.join("ok.rs");
        std::fs::write(&txt, "fn main() {}\n").unwrap();
        let bin = dir.join("blob.bin");
        std::fs::write(&bin, [0u8, 1, 0, 2]).unwrap();
        let abs = txt.to_str().unwrap().to_string();

        let t = claimable_target(&hit(&format!("{abs}:3:1"), "")).unwrap();
        assert_eq!(t.path, txt);
        assert_eq!(t.line, Some(3));
        assert_eq!(t.col, Some(1));

        let t = claimable_target(&hit("ok.rs", dir.to_str().unwrap())).unwrap();
        assert_eq!(t.path, txt);
        assert_eq!(t.line, None);

        let osc = format!("file://localhost{}", dir.display());
        let t = claimable_target(&hit("ok.rs", &osc)).unwrap();
        assert_eq!(t.path, txt);

        let binabs = bin.to_str().unwrap().to_string();
        assert!(claimable_target(&hit(&binabs, "")).is_none());
        assert!(claimable_target(&hit("/no/such/file.rs", "")).is_none());
        assert!(claimable_target(&hit("ok.rs", "")).is_none());
        assert!(claimable_target(&hit("https://x.io/a", "")).is_none());
        let _ = std::fs::remove_dir_all(&dir);
    }
}
