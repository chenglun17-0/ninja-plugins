//! 从进程 argv / 打开的 session 文件认出 pi、Codex、Claude，并拼出
//! 重启后应写入 PTY 的 resume 命令。
//!
//! 插件不把 Agent 知识送进协议：宿主只给 pane 槽位 + 前台 pid，本模块
//! 自己读 argv。

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use ninja_protocol::PaneInfo;
use serde::{Deserialize, Serialize};

/// 第一步支持的三个 CLI。以后加 agent 只改这里。
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum AgentKind {
    Pi,
    Codex,
    Claude,
}

impl AgentKind {
    pub fn as_str(self) -> &'static str {
        match self {
            AgentKind::Pi => "pi",
            AgentKind::Codex => "codex",
            AgentKind::Claude => "claude",
        }
    }

    fn bin(self) -> &'static str {
        self.as_str()
    }
}

/// 一个正在跑的 agent：要写进 persist 的最小信息。
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Recorded {
    pub kind: AgentKind,
    pub cwd: String,
    pub command: String,
}

/// 磁盘上的状态。`restored_host_pid` 防止同一 Ninja 进程里插件被热
/// 重载时把 resume 命令再打一遍。
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct Store {
    pub v: u32,
    #[serde(default)]
    pub restored_host_pid: u32,
    #[serde(default)]
    pub slots: BTreeMap<String, Recorded>,
}

pub const STORE_VERSION: u32 = 1;

const WRAPPERS: &[&str] = &[
    "node", "nodejs", "bun", "deno", "npx", "npm", "yarn", "pnpm", "env", "sudo", "nice", "time",
    "nohup",
];

const SHELLS: &[&str] = &[
    "zsh", "bash", "fish", "sh", "nu", "dash", "ksh", "login", "-zsh", "-bash", "-fish", "-sh",
];

pub fn slot_key(info: &PaneInfo) -> String {
    format!("{}.{}.{}", info.window, info.tab, info.leaf)
}

pub fn is_shell_comm(comm: &str) -> bool {
    let base = basename(comm);
    SHELLS.contains(&base)
}

pub fn store_path() -> PathBuf {
    if let Some(p) = std::env::var_os("NINJA_AGENT_RESTORE_STORE") {
        return PathBuf::from(p);
    }
    let home = std::env::var_os("HOME").unwrap_or_else(|| "/tmp".into());
    PathBuf::from(home)
        .join(".config")
        .join("ninja")
        .join("agent-restore.json")
}

pub fn load_store(path: &Path) -> Store {
    let Ok(text) = std::fs::read_to_string(path) else {
        return Store {
            v: STORE_VERSION,
            ..Store::default()
        };
    };
    match serde_json::from_str::<Store>(&text) {
        Ok(mut s) => {
            if s.v == 0 {
                s.v = STORE_VERSION;
            }
            s
        }
        Err(_) => Store {
            v: STORE_VERSION,
            ..Store::default()
        },
    }
}

pub fn save_store(path: &Path, store: &Store) -> std::io::Result<()> {
    if let Some(dir) = path.parent() {
        std::fs::create_dir_all(dir)?;
    }
    let tmp = path.with_extension("json.tmp");
    let bytes = serde_json::to_vec_pretty(store).map_err(std::io::Error::other)?;
    std::fs::write(&tmp, bytes)?;
    std::fs::rename(&tmp, path)
}

/// 从 NINJA_ADE_SOCK 文件名取出宿主 pid（`ninja-ade-{pid}.sock`）。
pub fn host_pid_from_sock(sock: &str) -> u32 {
    Path::new(sock)
        .file_stem()
        .and_then(|s| s.to_str())
        .and_then(|s| s.strip_prefix("ninja-ade-"))
        .and_then(|s| s.parse().ok())
        .unwrap_or(0)
}

/// 读 pid 的 argv；失败返回空。
pub fn proc_argv(pid: u32) -> Vec<String> {
    if pid == 0 {
        return Vec::new();
    }
    proc_argv_sysctl(pid).unwrap_or_default()
}

fn proc_argv_sysctl(pid: u32) -> Option<Vec<String>> {
    let pid = pid as i32;
    let mut mib = [libc::CTL_KERN, libc::KERN_PROCARGS2, pid];
    let mut buflen = 0usize;
    let rc = unsafe {
        libc::sysctl(
            mib.as_mut_ptr(),
            3,
            std::ptr::null_mut(),
            &mut buflen,
            std::ptr::null_mut(),
            0,
        )
    };
    if rc != 0 || buflen < 4 {
        return None;
    }
    let mut buf = vec![0u8; buflen];
    let rc = unsafe {
        libc::sysctl(
            mib.as_mut_ptr(),
            3,
            buf.as_mut_ptr().cast(),
            &mut buflen,
            std::ptr::null_mut(),
            0,
        )
    };
    if rc != 0 || buflen < 4 {
        return None;
    }
    buf.truncate(buflen);
    let argc = i32::from_ne_bytes(buf[0..4].try_into().ok()?);
    if argc <= 0 {
        return None;
    }
    let mut i = 4usize;
    while i < buf.len() && buf[i] != 0 {
        i += 1;
    }
    while i < buf.len() && buf[i] == 0 {
        i += 1;
    }
    let mut args = Vec::new();
    for _ in 0..argc {
        if i >= buf.len() {
            break;
        }
        let start = i;
        while i < buf.len() && buf[i] != 0 {
            i += 1;
        }
        if start < i {
            args.push(String::from_utf8_lossy(&buf[start..i]).into_owned());
        }
        i += 1;
    }
    if args.is_empty() {
        None
    } else {
        Some(args)
    }
}

fn basename(tok: &str) -> &str {
    Path::new(tok)
        .file_name()
        .and_then(|s| s.to_str())
        .unwrap_or(tok)
}

fn kind_from_token(tok: &str) -> Option<AgentKind> {
    let base = basename(tok);
    match base {
        "pi" | "pi-coding-agent" => Some(AgentKind::Pi),
        "codex" => Some(AgentKind::Codex),
        "claude" | "claude-code" => Some(AgentKind::Claude),
        "cli.js" | "cli.mjs" | "cli.cjs" => {
            if tok.contains("claude") {
                Some(AgentKind::Claude)
            } else if tok.contains("codex") {
                Some(AgentKind::Codex)
            } else if tok.contains("/pi") || tok.contains("pi-coding") {
                Some(AgentKind::Pi)
            } else {
                None
            }
        }
        _ => {
            if tok.contains("@anthropic-ai/claude") {
                Some(AgentKind::Claude)
            } else if tok.contains("@openai/codex") {
                Some(AgentKind::Codex)
            } else {
                None
            }
        }
    }
}

fn skip_wrapper(argv: &[String], mut i: usize) -> usize {
    while i < argv.len() {
        let base = basename(&argv[i]);
        if WRAPPERS.contains(&base) {
            i += 1;
            if base == "env" {
                while i < argv.len() && argv[i].contains('=') {
                    i += 1;
                }
            }
            continue;
        }
        break;
    }
    i
}

/// 在 argv 里找 agent 入口下标和种类。
pub fn detect_kind(argv: &[String]) -> Option<(usize, AgentKind)> {
    let mut i = skip_wrapper(argv, 0);
    while i < argv.len() {
        if let Some(kind) = kind_from_token(&argv[i]) {
            return Some((i, kind));
        }
        i = skip_wrapper(argv, i + 1);
    }
    None
}

fn flag_value<'a>(args: &'a [String], names: &[&str]) -> Option<&'a str> {
    let mut i = 0;
    while i < args.len() {
        for name in names {
            if args[i] == *name {
                return args.get(i + 1).map(|s| s.as_str());
            }
            let prefix = format!("{name}=");
            if let Some(v) = args[i].strip_prefix(&prefix) {
                return Some(v);
            }
        }
        i += 1;
    }
    None
}

fn positional_after<'a>(args: &'a [String], verb: &str) -> Option<&'a str> {
    let mut i = 0;
    while i < args.len() {
        if args[i] == verb {
            let mut j = i + 1;
            while j < args.len() {
                if args[j].starts_with('-') {
                    // --last / --all 等开关，跳过；带值的 -c key=value 也跳过。
                    if args[j] == "--last" || args[j] == "--all" || args[j].starts_with("--") {
                        if args[j].contains('=') {
                            j += 1;
                            continue;
                        }
                        // 长选项若下一 token 不像 id，当开关。
                        if let Some(next) = args.get(j + 1)
                            && !next.starts_with('-')
                            && !looks_like_id(next)
                        {
                            j += 2;
                            continue;
                        }
                        j += 1;
                        continue;
                    }
                    j += 1;
                    continue;
                }
                if looks_like_id(&args[j]) {
                    return Some(args[j].as_str());
                }
                return None;
            }
            return None;
        }
        i += 1;
    }
    None
}

fn looks_like_id(s: &str) -> bool {
    let t = s.trim();
    if t.len() < 8 {
        return false;
    }
    find_uuid(t).is_some() || t.chars().all(|c| c.is_ascii_hexdigit() || c == '-')
}

fn is_uuid(s: &str) -> bool {
    let b = s.as_bytes();
    if b.len() != 36 {
        return false;
    }
    let dash = |i: usize| b[i] == b'-';
    let hex_run = |a: usize, n: usize| b[a..a + n].iter().all(u8::is_ascii_hexdigit);
    dash(8)
        && dash(13)
        && dash(18)
        && dash(23)
        && hex_run(0, 8)
        && hex_run(9, 4)
        && hex_run(14, 4)
        && hex_run(19, 4)
        && hex_run(24, 12)
}

fn find_uuid(s: &str) -> Option<&str> {
    const LEN: usize = 36;
    if s.len() < LEN {
        return None;
    }
    for i in 0..=s.len() - LEN {
        let cand = &s[i..i + LEN];
        if is_uuid(cand) {
            return Some(cand);
        }
    }
    None
}

fn id_from_path(path: &Path) -> Option<String> {
    let name = path.file_name()?.to_str()?;
    if let Some(id) = find_uuid(name) {
        return Some(id.to_string());
    }
    let stem = path.file_stem()?.to_str()?;
    if let Some((_, tail)) = stem.rsplit_once('_')
        && looks_like_id(tail)
    {
        return Some(tail.to_string());
    }
    None
}

fn jsonl_from_pid(pid: u32) -> Vec<PathBuf> {
    if pid == 0 {
        return Vec::new();
    }
    let out = std::process::Command::new("lsof")
        .args(["-a", "-p", &pid.to_string(), "-Fn"])
        .output();
    let Ok(out) = out else {
        return Vec::new();
    };
    let text = String::from_utf8_lossy(&out.stdout);
    let mut files = Vec::new();
    for line in text.lines() {
        let Some(path) = line.strip_prefix('n') else {
            continue;
        };
        if path.ends_with(".jsonl") {
            files.push(PathBuf::from(path));
        }
    }
    files
}

fn session_id_from_open_files(pid: u32) -> Option<String> {
    for path in jsonl_from_pid(pid) {
        if let Some(id) = id_from_path(&path) {
            return Some(id);
        }
    }
    None
}

fn latest_pi_session(cwd: &str) -> Option<String> {
    if cwd.is_empty() {
        return None;
    }
    let home = std::env::var_os("HOME")?;
    let encoded = format!("--{}--", cwd.trim_start_matches('/').replace('/', "-"));
    let dir = PathBuf::from(home)
        .join(".pi")
        .join("agent")
        .join("sessions")
        .join(encoded);
    latest_jsonl_id(&dir)
}

fn latest_jsonl_id(dir: &Path) -> Option<String> {
    let mut best: Option<(std::time::SystemTime, PathBuf)> = None;
    let rd = std::fs::read_dir(dir).ok()?;
    for ent in rd.flatten() {
        let path = ent.path();
        if path.extension().and_then(|s| s.to_str()) != Some("jsonl") {
            continue;
        }
        let mtime = ent.metadata().ok().and_then(|m| m.modified().ok())?;
        match &best {
            Some((t, _)) if *t >= mtime => {}
            _ => best = Some((mtime, path)),
        }
    }
    best.and_then(|(_, p)| id_from_path(&p))
}

fn resume_command(kind: AgentKind, session: &str) -> String {
    match kind {
        AgentKind::Pi => format!("{} --session {session}", kind.bin()),
        AgentKind::Codex => format!("{} resume {session}", kind.bin()),
        AgentKind::Claude => format!("{} --resume {session}", kind.bin()),
    }
}

fn sh_single_quote(s: &str) -> String {
    format!("'{}'", s.replace('\'', "'\\''"))
}

/// 写入 PTY 的整行：先 cd 到记录的目录（macOS .app 进程 cwd 是 `/`），
/// 再跑 resume。cwd 空则只发命令。
pub fn inject_script(cwd: &str, command: &str) -> String {
    let cmd = command.trim_end_matches(['\n', '\r']);
    if cwd.is_empty() {
        format!("{cmd}\n")
    } else {
        format!("cd {} && {cmd}\n", sh_single_quote(cwd))
    }
}

/// 从 argv + 可选 pid/cwd 拼出一条可恢复的记录。认不出 session 则 None
///（宁可这次不恢复，也不要 `pi --continue` 拉错对话）。
pub fn record_from_argv(argv: &[String], pid: u32, cwd: &str) -> Option<Recorded> {
    let (idx, kind) = detect_kind(argv)?;
    let rest = &argv[idx + 1..];
    let session = match kind {
        AgentKind::Pi => flag_value(rest, &["--session", "--session-id"])
            .map(str::to_string)
            .or_else(|| session_id_from_open_files(pid))
            .or_else(|| latest_pi_session(cwd)),
        AgentKind::Codex => positional_after(rest, "resume")
            .map(str::to_string)
            .or_else(|| session_id_from_open_files(pid)),
        AgentKind::Claude => flag_value(rest, &["--resume", "-r"])
            .map(str::to_string)
            .or_else(|| session_id_from_open_files(pid)),
    }?;
    let session = session.trim();
    if session.is_empty() {
        return None;
    }
    Some(Recorded {
        kind,
        cwd: cwd.to_string(),
        command: resume_command(kind, session),
    })
}

pub fn record_from_pid(pid: u32, cwd: &str) -> Option<Recorded> {
    let argv = proc_argv(pid);
    if argv.is_empty() {
        return None;
    }
    record_from_argv(&argv, pid, cwd)
}

pub fn comm_from_argv(argv: &[String]) -> &str {
    argv.first().map(|s| basename(s)).unwrap_or("")
}

/// 这个 pane 现在看起来像空闲 shell，可以往里面打 resume 命令。
pub fn pane_is_idle_shell(info: &PaneInfo, argv: &[String]) -> bool {
    if info.fg_pid == 0 {
        return false;
    }
    if detect_kind(argv).is_some() {
        return false;
    }
    is_shell_comm(comm_from_argv(argv))
}

/// 把 pending 槽位对到当前快照：先精确槽位，再唯一 cwd。
pub fn match_pane<'a>(
    slot: &str,
    cwd: &str,
    panes: &'a [PaneInfo],
) -> Option<&'a PaneInfo> {
    if let Some(p) = panes.iter().find(|p| slot_key(p) == slot) {
        return Some(p);
    }
    if cwd.is_empty() {
        return None;
    }
    let hits: Vec<&PaneInfo> = panes.iter().filter(|p| p.cwd == cwd).collect();
    if hits.len() == 1 {
        Some(hits[0])
    } else {
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use ninja_protocol::PaneInfo;

    fn rec(argv: &[&str], cwd: &str) -> Option<Recorded> {
        let argv: Vec<String> = argv.iter().map(|s| (*s).to_string()).collect();
        record_from_argv(&argv, 0, cwd)
    }

    #[test]
    fn detects_plain_pi_session_flag() {
        let r = rec(
            &["pi", "--session", "01a02485-af31-7e7b-811f-27581c589da9"],
            "/tmp",
        )
        .unwrap();
        assert_eq!(r.kind, AgentKind::Pi);
        assert_eq!(
            r.command,
            "pi --session 01a02485-af31-7e7b-811f-27581c589da9"
        );
    }

    #[test]
    fn detects_pi_equals_form_and_node_wrapper() {
        let r = rec(
            &[
                "node",
                "/Users/jal/.nvm/versions/node/v24.18.0/lib/node_modules/@mariozechner/pi-coding-agent/dist/cli.js",
                "--session=01a02485-af31-7e7b-811f-27581c589da9",
            ],
            "/tmp",
        )
        .unwrap();
        assert_eq!(r.kind, AgentKind::Pi);
        assert_eq!(
            r.command,
            "pi --session 01a02485-af31-7e7b-811f-27581c589da9"
        );
    }

    #[test]
    fn detects_codex_resume_uuid() {
        let r = rec(
            &[
                "codex",
                "resume",
                "019daa50-8665-7140-bb57-8cd13045d98d",
            ],
            "/tmp",
        )
        .unwrap();
        assert_eq!(r.kind, AgentKind::Codex);
        assert_eq!(
            r.command,
            "codex resume 019daa50-8665-7140-bb57-8cd13045d98d"
        );
    }

    #[test]
    fn detects_claude_resume() {
        let r = rec(
            &["claude", "--resume", "b2361929-792f-4de3-b267-a5b299bea19b"],
            "/tmp",
        )
        .unwrap();
        assert_eq!(r.kind, AgentKind::Claude);
        assert_eq!(
            r.command,
            "claude --resume b2361929-792f-4de3-b267-a5b299bea19b"
        );
    }

    #[test]
    fn ignores_non_agent() {
        assert!(rec(&["vim", "main.rs"], "/tmp").is_none());
        assert!(rec(&["zsh"], "/tmp").is_none());
        assert!(rec(&["git", "log"], "/tmp").is_none());
    }

    #[test]
    fn pi_without_session_is_skipped() {
        assert!(rec(&["pi"], "/no/such/cwd").is_none());
    }

    #[test]
    fn slot_and_match() {
        let a = PaneInfo::new(1, 0, 0, 0, "/a", 10);
        let b = PaneInfo::new(2, 0, 1, 0, "/b", 11);
        let panes = [a.clone(), b.clone()];
        assert_eq!(slot_key(&a), "0.0.0");
        assert_eq!(match_pane("0.1.0", "/b", &panes).unwrap().pane, 2);
        assert_eq!(match_pane("9.9.9", "/a", &panes).unwrap().pane, 1);
        assert!(match_pane("9.9.9", "/missing", &panes).is_none());
        let dup = [
            PaneInfo::new(1, 0, 0, 0, "/same", 1),
            PaneInfo::new(2, 0, 1, 0, "/same", 2),
        ];
        assert!(match_pane("9.9.9", "/same", &dup).is_none());
    }

    #[test]
    fn idle_shell_vs_agent() {
        let pane = PaneInfo::new(1, 0, 0, 0, "/tmp", 42);
        let zsh = vec!["/bin/zsh".into()];
        let pi = vec!["pi".into(), "--session".into(), "abc".into()];
        assert!(pane_is_idle_shell(&pane, &zsh));
        assert!(!pane_is_idle_shell(&pane, &pi));
        let mut empty = pane.clone();
        empty.fg_pid = 0;
        assert!(!pane_is_idle_shell(&empty, &zsh));
    }

    #[test]
    fn id_from_known_filenames() {
        assert_eq!(
            id_from_path(Path::new(
                "2026-08-21T13-32-16-049Z_01a02485-af31-7e7b-811f-27581c589da9.jsonl"
            ))
            .as_deref(),
            Some("01a02485-af31-7e7b-811f-27581c589da9")
        );
        assert_eq!(
            id_from_path(Path::new(
                "rollout-2026-04-20T17-54-57-019daa50-8665-7140-bb57-8cd13045d98d.jsonl"
            ))
            .as_deref(),
            Some("019daa50-8665-7140-bb57-8cd13045d98d")
        );
        assert_eq!(
            id_from_path(Path::new("b2361929-792f-4de3-b267-a5b299bea19b.jsonl"))
                .as_deref(),
            Some("b2361929-792f-4de3-b267-a5b299bea19b")
        );
    }

    #[test]
    fn host_pid_parses_sock_name() {
        assert_eq!(
            host_pid_from_sock("/var/folders/xx/T/ninja-ade-12345.sock"),
            12345
        );
        assert_eq!(host_pid_from_sock("/tmp/other.sock"), 0);
    }

    #[test]
    fn store_roundtrip() {
        let dir = std::env::temp_dir().join(format!("ninja-agent-restore-test-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("agent-restore.json");
        let mut s = Store {
            v: STORE_VERSION,
            restored_host_pid: 9,
            slots: BTreeMap::new(),
        };
        s.slots.insert(
            "0.0.0".into(),
            Recorded {
                kind: AgentKind::Pi,
                cwd: "/tmp".into(),
                command: "pi --session abc".into(),
            },
        );
        save_store(&path, &s).unwrap();
        let back = load_store(&path);
        assert_eq!(back, s);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn inject_script_cds_then_runs() {
        assert_eq!(
            inject_script("/Users/jal/repos/ecloud_agent", "pi --session abc"),
            "cd '/Users/jal/repos/ecloud_agent' && pi --session abc\n"
        );
        assert_eq!(inject_script("", "pi --session abc"), "pi --session abc\n");
        assert_eq!(
            inject_script("/tmp/it's", "pi --session x"),
            "cd '/tmp/it'\\''s' && pi --session x\n",
        );
    }
}

