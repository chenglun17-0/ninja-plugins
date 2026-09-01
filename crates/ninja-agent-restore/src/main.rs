//! ninja-agent-restore：记录当前窗口里跑着的 CLI agent，Ninja 关掉再开
//! 时自动把 `pi --session …` / `codex resume …` / `claude --resume …`
//! 打回对应 PTY。
//!
//! 独立进程，只经 ADE 协议说话：
//!
//! ```text
//! 宿主 spawn（env NINJA_ADE_SOCK）→ connect
//! ← pane.snapshot   各终端面的槽位 / cwd / 前台 pid
//!    认出 pi/codex/claude → 写入 ~/.config/ninja/agent-restore.json
//!    若本宿主尚未恢复过 → 空闲 shell 上 pane.input 打 resume 命令
//! ← hit             → hit.ignore（本插件不认领路径）
//! ← EOF             宿主退出（正常收尾）
//! ```
//!
//! 窗口几何/标签/工作目录仍由宿主 `window-save-state` 恢复；本插件只
//! 补「当时在跑的 agent」。agent 在关 Ninja 前已经退回 shell 的，不恢复。

mod agent;

use std::collections::{BTreeMap, BTreeSet};

/// 每 pane 的上次观察状态（边沿检测用：pid/cwd/slot 任一变化都重试记录）。
#[derive(Default)]
struct PaneSeen {
    pid: BTreeMap<u32, u32>,
    cwd: BTreeMap<u32, String>,
    slot: BTreeMap<u32, String>,
}
use std::io::{Read, Write};
use std::os::unix::net::UnixStream;
use std::time::Instant;

use ninja_protocol::frame::{encode_frame, FrameDecoder};
use ninja_protocol::{DecodeError, HitIgnore, Message, PaneInfo, PaneInput, PaneSnapshot};

use crate::agent::{
    host_pid_from_sock, inject_script, load_store, match_pane, pane_is_idle_shell, proc_argv,
    record_from_pid, save_store, slot_key, store_path, Store,
};

const SOCK_ENV: &str = "NINJA_ADE_SOCK";
/// 等 shell 起来再打命令，避免跟 .zshrc 抢 tty。
const RESTORE_GRACE: std::time::Duration = std::time::Duration::from_millis(400);
/// 冷启动后这段时间内一直重试注入（不依赖 pid 再变一次才有快照）。
const RESTORE_WINDOW: std::time::Duration = std::time::Duration::from_secs(8);
const SOCK_POLL: std::time::Duration = std::time::Duration::from_millis(100);

fn main() {
    std::process::exit(run());
}

fn run() -> i32 {
    let Some(sock) = std::env::var_os(SOCK_ENV) else {
        eprintln!("ninja-agent-restore: 缺 {SOCK_ENV}（应由宿主拉起）");
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
        eprintln!("ninja-agent-restore: 连不上 ADE socket {sock:?}");
        return 2;
    };
    eprintln!("ninja-agent-restore: 已连接宿主（v0）");
    let _ = stream.set_read_timeout(Some(SOCK_POLL));

    let sock_s = sock.to_string_lossy().into_owned();
    let host_pid = host_pid_from_sock(&sock_s);
    let path = store_path();
    let mut store = load_store(&path);
    let mut pending: Vec<(String, String, String)> = Vec::new(); // slot, cwd, command
    if store.restored_host_pid != host_pid {
        store.restored_host_pid = host_pid;
        let _ = save_store(&path, &store);
        pending = store
            .slots
            .iter()
            .map(|(slot, rec)| (slot.clone(), rec.cwd.clone(), rec.command.clone()))
            .collect();
        if !pending.is_empty() {
            eprintln!(
                "ninja-agent-restore: 本宿主 pid={host_pid} 待恢复 {} 个 agent",
                pending.len()
            );
        }
    }

    let mut decoder = FrameDecoder::new();
    let mut buf = [0u8; 8192];
    let started = Instant::now();
    let mut saw_panes = false;
    let mut seen = PaneSeen::default();
    let mut last_panes: Vec<PaneInfo> = Vec::new();
    loop {
        match stream.read(&mut buf) {
            Ok(0) => {
                let _ = save_store(&path, &store);
                return 0;
            }
            Ok(n) => {
                if decoder.extend(&buf[..n]).is_err() {
                    eprintln!("ninja-agent-restore: 帧缓冲超限，断开");
                    return 2;
                }
            }
            Err(e)
                if e.kind() == std::io::ErrorKind::Interrupted
                    || e.kind() == std::io::ErrorKind::WouldBlock
                    || e.kind() == std::io::ErrorKind::TimedOut =>
            {
                if !pending.is_empty()
                    && started.elapsed() >= RESTORE_GRACE
                    && started.elapsed() < RESTORE_WINDOW
                    && !last_panes.is_empty()
                {
                    retry_pending(&mut pending, &last_panes, &mut stream);
                }
                continue;
            }
            Err(e) => {
                eprintln!("ninja-agent-restore: socket 读失败：{e}");
                return 2;
            }
        }
        while let Some(payload) = decoder.pop() {
            let payload = match payload {
                Ok(p) => p,
                Err(e) => {
                    eprintln!("ninja-agent-restore: 帧级违规（{e}），断开");
                    return 2;
                }
            };
            match Message::decode_plugin(&payload) {
                Ok(Message::PaneSnapshot(snap)) => {
                    last_panes = snap.panes.clone();
                    on_snapshot(
                        &snap,
                        &mut store,
                        &path,
                        &mut pending,
                        &mut saw_panes,
                        &mut seen,
                        started,
                        &mut stream,
                    );
                }
                Ok(Message::Hit(hit)) => {
                    let reply = encode_frame(&Message::HitIgnore(HitIgnore::new(hit.id)))
                        .expect("hit.ignore 编码");
                    if stream.write_all(&reply).is_err() {
                        return 0;
                    }
                }
                Ok(_) => {}
                Err(DecodeError::UnsupportedVersion { got, supported }) => {
                    eprintln!(
                        "ninja-agent-restore: 协议版本 v{got} 不支持（本实现 v{supported}），退出"
                    );
                    return 78;
                }
                Err(e) => {
                    eprintln!("ninja-agent-restore: 丢弃无法解码的消息：{e}");
                }
            }
        }
    }
}

// run-loop 状态直传（快照+存储+恢复窗+连接）；聚成 ctx 结构只是搬运同
// 一批字段，不减少耦合。
#[allow(clippy::too_many_arguments)]
fn on_snapshot(
    snap: &PaneSnapshot,
    store: &mut Store,
    path: &std::path::Path,
    pending: &mut Vec<(String, String, String)>,
    saw_panes: &mut bool,
    seen: &mut PaneSeen,
    started: Instant,
    stream: &mut UnixStream,
) {
    if snap.panes.is_empty() {
        return;
    }

    if !*saw_panes {
        *saw_panes = true;
        // 冷启动第一份快照可能还没把全部 tab 建完，不能把 pending 扔掉。
    }

    let mut dirty = false;
    let live_keys: BTreeSet<String> = snap.panes.iter().map(slot_key).collect();
    let live_panes: BTreeSet<u32> = snap.panes.iter().map(|p| p.pane).collect();
    seen.pid.retain(|pane, _| live_panes.contains(pane));
    seen.cwd.retain(|pane, _| live_panes.contains(pane));
    seen.slot.retain(|pane, _| live_panes.contains(pane));

    for pane in &snap.panes {
        if std::env::var_os("NINJA_AR_DEBUG").is_some() {
            eprintln!(
                "ninja-agent-restore: snap pane={} slot={} fg_pid={} cwd={:?}",
                pane.pane,
                slot_key(pane),
                pane.fg_pid,
                pane.cwd
            );
        }
        let key = slot_key(pane);
        let prev_slot = seen.slot.insert(pane.pane, key.clone());
        let pid_changed = seen.pid.insert(pane.pane, pane.fg_pid) != Some(pane.fg_pid);
        // cwd 迟到（OSC-7/前台 cwd 兜底晚于 fg 切换）也要重试记录：
        // 边沿只看 pid 会把「fg=agent 但 cwd 还空」的窗永久错过。
        let cwd_changed = seen.cwd.insert(pane.pane, pane.cwd.clone()).as_deref()
            != Some(pane.cwd.as_str());

        if !pid_changed && !cwd_changed {
            if let Some(old) = prev_slot
                && old != key
                && let Some(rec) = store.slots.remove(&old)
            {
                store.slots.insert(key.clone(), rec);
                dirty = true;
            }
            continue;
        }

        if let Some(rec) = record_from_pid(pane.fg_pid, &pane.cwd) {
            if store.slots.get(&key) != Some(&rec) {
                eprintln!(
                    "ninja-agent-restore: 记录 {} {} @ {}",
                    rec.kind.as_str(),
                    key,
                    pane.cwd
                );
                store.slots.insert(key.clone(), rec);
                dirty = true;
            }
            pending.retain(|(s, _, _)| s != &key);
            continue;
        }
        let pending_here = pending.iter().any(|(s, cwd, _)| {
            s == &key || (!cwd.is_empty() && cwd == &pane.cwd)
        });
        if !pending_here && store.slots.remove(&key).is_some() {
            dirty = true;
        }
    }

    if started.elapsed() > RESTORE_WINDOW {
        store.slots.retain(|k, rec| {
            live_keys.contains(k) || snap.panes.iter().any(|p| p.cwd == rec.cwd)
        });
    }

    if started.elapsed() >= RESTORE_GRACE {
        retry_pending(pending, &snap.panes, stream);
    }

    if dirty {
        let _ = save_store(path, store);
    }
}

fn retry_pending(
    pending: &mut Vec<(String, String, String)>,
    panes: &[PaneInfo],
    stream: &mut UnixStream,
) {
    let mut claimed = std::collections::BTreeSet::new();
    let mut still = Vec::new();
    for (slot, cwd, command) in pending.drain(..) {
        match try_inject(stream, panes, &slot, &cwd, &command, &mut claimed) {
            Inject::Done => {}
            Inject::Wait => still.push((slot, cwd, command)),
        }
    }
    *pending = still;
}

enum Inject {
    Done,
    Wait,
}

fn try_inject(
    stream: &mut UnixStream,
    panes: &[PaneInfo],
    slot: &str,
    cwd: &str,
    command: &str,
    claimed: &mut std::collections::BTreeSet<u32>,
) -> Inject {
    let Some(pane) = match_pane(slot, cwd, panes, claimed) else {
        return Inject::Wait;
    };
    // 匹配即认领（无论随后是注入还是「已是该 agent」）：同 cwd 的其他
    // 记录不会再挑中这个 pane。
    claimed.insert(pane.pane);
    let argv = proc_argv(pane.fg_pid);
    if !pane_is_idle_shell(pane, &argv) {
        if argv.is_empty() {
            return Inject::Wait;
        }
        // 已经是这个 agent，或用户在跑别的东西：不再打。
        return Inject::Done;
    }
    let text = inject_script(cwd, command);
    let msg = Message::PaneInput(PaneInput::new(pane.pane, text));
    match encode_frame(&msg) {
        Ok(frame) => {
            if stream.write_all(&frame).is_err() {
                return Inject::Wait;
            }
            eprintln!(
                "ninja-agent-restore: 恢复 pane={} slot={slot} `{command}`",
                pane.pane
            );
            Inject::Done
        }
        Err(e) => {
            eprintln!("ninja-agent-restore: pane.input 编码失败：{e}");
            Inject::Done
        }
    }
}
