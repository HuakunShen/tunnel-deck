#![forbid(unsafe_code)]
//! Default host implementations: processes, TCP probes, files, control
//! sockets, terminals, URLs, and ssh-config inventory. Each is a small struct
//! over injected paths; the manager owns the policy, these own the syscalls.
//! Mirrors the `packages/ssh-host-node` implementations.

use crate::contract::SshHostInfo;
use crate::contract::TunnelProfile;
use crate::error::{Result, TunnelError, TunnelProblemCode};
use crate::ports::{
    ControlPort, LiveChild, OpenUrlPort, ProbeOutcome, ProbePort, ProcessPort, RuntimeRecord,
    RuntimeStorePort, StorePort, TerminalInfo, TerminalPort,
};
use std::fs::{self, OpenOptions};
use std::io::Write;
use std::net::{SocketAddr, TcpStream, ToSocketAddrs};
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::time::{Duration, Instant};

/// Default base directory: `~/.ssh-tunnel-ui`.
pub fn default_base_dir() -> PathBuf {
    std::env::var_os("HOME")
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from("."))
        .join(".ssh-tunnel-ui")
}

// --- processes ---------------------------------------------------------------

/// Spawn system `ssh` detached with stdout/stderr appended to a per-tunnel log
/// file. No shell is ever involved: argv passes straight to `spawn`.
pub struct OsProcess {
    pub ssh_path: String,
}

impl OsProcess {
    pub fn new(ssh_path: impl Into<String>) -> Self {
        Self {
            ssh_path: ssh_path.into(),
        }
    }
}

impl ProcessPort for OsProcess {
    fn spawn(&mut self, argv: &[String], log_file: &Path) -> Result<LiveChild> {
        if let Some(parent) = log_file.parent() {
            let _ = fs::create_dir_all(parent);
        }
        let log = OpenOptions::new()
            .create(true)
            .append(true)
            .open(log_file)
            .map_err(|e| {
                TunnelError::new(
                    TunnelProblemCode::SshFailed,
                    format!("cannot open log file: {e}"),
                )
            })?;
        let err_log = log.try_clone().map_err(|e| {
            TunnelError::new(
                TunnelProblemCode::SshFailed,
                format!("cannot open log file: {e}"),
            )
        })?;
        let mut command = Command::new(&self.ssh_path);
        command
            .args(argv)
            .stdin(Stdio::null())
            .stdout(Stdio::from(log))
            .stderr(Stdio::from(err_log));
        #[cfg(unix)]
        {
            use std::os::unix::process::CommandExt as _;
            command.process_group(0);
        }
        let child: Child = command.spawn().map_err(|e| {
            if e.kind() == std::io::ErrorKind::NotFound {
                TunnelError::new(
                    TunnelProblemCode::SshNotFound,
                    format!("ssh executable not found at {}", self.ssh_path),
                )
            } else {
                TunnelError::new(
                    TunnelProblemCode::SshFailed,
                    format!("cannot spawn ssh: {e}"),
                )
            }
        })?;
        let pid = child.id();
        if pid == 0 {
            return Err(TunnelError::new(
                TunnelProblemCode::SshNotFound,
                "ssh executable not found",
            ));
        }
        Ok(LiveChild {
            pid,
            child: Some(child),
        })
    }

    fn is_alive(&self, pid: u32) -> bool {
        is_pid_alive(pid)
    }

    fn kill_tree(&mut self, pid: u32) {
        kill_pid(pid);
    }
}

/// True while pid answers, via `kill -0` (signal zero performs no action).
/// A missing `kill` binary answers conservatively: assume alive rather than
/// mark a running tunnel failed for lack of a probe.
#[cfg(unix)]
pub fn is_pid_alive(pid: u32) -> bool {
    if pid == 0 {
        return false;
    }
    match Command::new("kill").args(["-0", &pid.to_string()]).output() {
        Ok(output) => output.status.success(),
        Err(_) => true,
    }
}

#[cfg(windows)]
pub fn is_pid_alive(pid: u32) -> bool {
    if pid == 0 {
        return false;
    }
    Command::new("tasklist")
        .args(["/FI", &format!("PID eq {pid}"), "/NH"])
        .output()
        .is_ok_and(|out| String::from_utf8_lossy(&out.stdout).contains(&pid.to_string()))
}

#[cfg(unix)]
fn kill_pid(pid: u32) {
    // Failure means already gone; the caller re-checks liveness anyway.
    let _ = Command::new("kill")
        .args(["-TERM", &pid.to_string()])
        .output();
}

#[cfg(windows)]
fn kill_pid(pid: u32) {
    let _ = Command::new("taskkill")
        .args(["/PID", &pid.to_string(), "/T", "/F"])
        .output();
}

// --- probes ------------------------------------------------------------------

pub struct TcpProbe;

impl ProbePort for TcpProbe {
    fn dial_tcp(&self, host: &str, port: u16, timeout_ms: u64) -> ProbeOutcome {
        let started = Instant::now();
        let timeout = Duration::from_millis(timeout_ms);
        let addrs: Vec<SocketAddr> = match (host, port).to_socket_addrs() {
            Ok(addrs) => addrs.collect(),
            Err(e) => {
                return ProbeOutcome {
                    ok: false,
                    latency_ms: None,
                    detail: Some(e.to_string()),
                };
            }
        };
        for addr in addrs {
            match TcpStream::connect_timeout(&addr, timeout) {
                Ok(_) => {
                    return ProbeOutcome {
                        ok: true,
                        latency_ms: Some(started.elapsed().as_millis().min(u64::MAX as u128) as u64),
                        detail: None,
                    };
                }
                Err(e) => {
                    if e.kind() != std::io::ErrorKind::ConnectionRefused
                        && e.kind() != std::io::ErrorKind::TimedOut
                    {
                        continue;
                    }
                }
            }
        }
        ProbeOutcome {
            ok: false,
            latency_ms: None,
            detail: Some("connection refused".into()),
        }
    }
}

// --- files -------------------------------------------------------------------

pub struct FileStore {
    pub path: PathBuf,
}

impl FileStore {
    pub fn new(path: PathBuf) -> Self {
        Self { path }
    }
}

impl StorePort for FileStore {
    fn read_profiles(&self) -> Result<Vec<TunnelProfile>> {
        let Ok(raw) = fs::read_to_string(&self.path) else {
            return Ok(Vec::new());
        };
        // A corrupt store degrades to empty, never to a failed boot.
        Ok(serde_json::from_str(&raw).unwrap_or_default())
    }

    fn write_profiles(&self, profiles: &[TunnelProfile]) -> Result<()> {
        if let Some(parent) = self.path.parent() {
            let _ = fs::create_dir_all(parent);
        }
        let raw = serde_json::to_string_pretty(profiles).map_err(|e| {
            TunnelError::new(
                TunnelProblemCode::StoreFailed,
                format!("cannot encode profiles: {e}"),
            )
        })?;
        fs::write(&self.path, raw).map_err(|e| {
            TunnelError::new(
                TunnelProblemCode::StoreFailed,
                format!("cannot write profiles: {e}"),
            )
        })
    }
}

pub struct FileRuntimeStore {
    pub path: PathBuf,
}

impl FileRuntimeStore {
    pub fn new(path: PathBuf) -> Self {
        Self { path }
    }
}

impl RuntimeStorePort for FileRuntimeStore {
    fn read(&self) -> Vec<RuntimeRecord> {
        let Ok(raw) = fs::read_to_string(&self.path) else {
            return Vec::new();
        };
        serde_json::from_str(&raw).unwrap_or_default()
    }

    fn write(&self, records: &[RuntimeRecord]) {
        if let Some(parent) = self.path.parent() {
            let _ = fs::create_dir_all(parent);
        }
        let Ok(raw) = serde_json::to_string_pretty(records) else {
            return;
        };
        // Best-effort by contract: a failed write must never fail the operation.
        let _ = fs::write(&self.path, raw);
    }
}

// --- control sockets -----------------------------------------------------------

/// How long `ssh -O` may take before the socket is treated as gone.
const CONTROL_TIMEOUT_SECS: u64 = 2;

/// `ssh -O check` / `ssh -O exit` against one ControlMaster socket. Nothing
/// here runs a shell; argv goes straight to `spawn`.
pub struct SshControl {
    pub ssh_path: String,
}

impl SshControl {
    pub fn new(ssh_path: impl Into<String>) -> Self {
        Self {
            ssh_path: ssh_path.into(),
        }
    }

    fn run(&self, action: &str, socket_path: &Path, destination: &str) -> bool {
        let mut child = match Command::new(&self.ssh_path)
            .args(["-o", "BatchMode=yes", "-S"])
            .arg(socket_path)
            .args(["-O", action, destination])
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn()
        {
            Ok(child) => child,
            Err(_) => return false,
        };
        let deadline = Instant::now() + Duration::from_secs(CONTROL_TIMEOUT_SECS);
        loop {
            match child.try_wait() {
                Ok(Some(status)) => return status.success(),
                Ok(None) => {
                    if Instant::now() >= deadline {
                        let _ = child.kill();
                        return false;
                    }
                    std::thread::sleep(Duration::from_millis(50));
                }
                Err(_) => return false,
            }
        }
    }
}

impl ControlPort for SshControl {
    fn check(&self, socket_path: &Path, destination: &str) -> bool {
        if !socket_path.exists() {
            return false;
        }
        self.run("check", socket_path, destination)
    }

    fn exit(&self, socket_path: &Path, destination: &str) -> bool {
        self.run("exit", socket_path, destination)
    }
}

/// Can this host's OpenSSH multiplex sessions at all? The Windows build has
/// no `ControlMaster`, so a tunnel there is owned by pid instead.
pub fn supports_control_master() -> bool {
    !cfg!(windows)
}

// --- terminals -----------------------------------------------------------------

/// PATH lookup for terminal launchers. Used on Linux and Windows; macOS
/// proves installation through the app bundle instead.
#[allow(dead_code)]
fn which_on_path(command: &str) -> Option<PathBuf> {
    let path = std::env::var_os("PATH")?;
    for dir in std::env::split_paths(&path) {
        if dir.as_os_str().is_empty() {
            continue;
        }
        #[cfg(windows)]
        {
            for name in [format!("{command}.exe"), command.to_string()] {
                let candidate = dir.join(&name);
                if candidate.is_file() {
                    return Some(candidate);
                }
            }
        }
        #[cfg(not(windows))]
        {
            let candidate = dir.join(command);
            if candidate.is_file() {
                return Some(candidate);
            }
        }
    }
    None
}

#[cfg(target_os = "macos")]
fn mac_bundle_executable(app: &str) -> Option<PathBuf> {
    let home = std::env::var_os("HOME").map(PathBuf::from);
    let mut dirs = vec![
        PathBuf::from("/Applications"),
        PathBuf::from("/System/Applications"),
        PathBuf::from("/System/Applications/Utilities"),
    ];
    if let Some(home) = home {
        dirs.push(home.join("Applications"));
    }
    for dir in dirs {
        let macos = dir
            .join(format!("{app}.app"))
            .join("Contents")
            .join("MacOS");
        let entries = fs::read_dir(&macos).ok()?;
        for entry in entries.flatten() {
            let path = entry.path();
            if path.is_file() {
                return Some(path);
            }
        }
    }
    None
}

#[cfg(target_os = "macos")]
struct TerminalTarget {
    id: &'static str,
    label: &'static str,
    hint: &'static str,
    platform: &'static str,
    bundle_apps: &'static [&'static str],
}

#[cfg(target_os = "macos")]
const MAC_TARGETS: &[TerminalTarget] = &[
    TerminalTarget {
        id: "terminal",
        label: "Terminal",
        hint: "com.apple.Terminal",
        platform: "darwin",
        bundle_apps: &["Terminal"],
    },
    TerminalTarget {
        id: "iterm2",
        label: "iTerm2",
        hint: "com.googlecode.iterm2",
        platform: "darwin",
        bundle_apps: &["iTerm", "iTerm2"],
    },
    TerminalTarget {
        id: "warp",
        label: "Warp",
        hint: "dev.warp.Warp-Stable",
        platform: "darwin",
        bundle_apps: &["Warp", "WarpPreview"],
    },
    TerminalTarget {
        id: "ghostty",
        label: "Ghostty",
        hint: "com.mitchellh.ghostty",
        platform: "darwin",
        bundle_apps: &["Ghostty"],
    },
    TerminalTarget {
        id: "kitty",
        label: "kitty",
        hint: "kitty",
        platform: "darwin",
        bundle_apps: &["kitty"],
    },
    TerminalTarget {
        id: "wezterm",
        label: "WezTerm",
        hint: "com.github.wez.wezterm",
        platform: "darwin",
        bundle_apps: &["WezTerm"],
    },
    TerminalTarget {
        id: "alacritty",
        label: "Alacritty",
        hint: "org.alacritty",
        platform: "darwin",
        bundle_apps: &["Alacritty"],
    },
    TerminalTarget {
        id: "vscode",
        label: "VS Code",
        hint: "com.microsoft.VSCode",
        platform: "darwin",
        bundle_apps: &["Visual Studio Code"],
    },
];

#[cfg(target_os = "linux")]
const LINUX_TARGETS: &[(&str, &str, &str)] = &[
    ("gnome-terminal", "GNOME Terminal", "gnome-terminal"),
    ("konsole", "Konsole", "org.kde.konsole"),
    ("alacritty", "Alacritty", "org.alacritty"),
    ("kitty", "kitty", "kitty"),
    ("xterm", "XTerm", "xterm"),
];

#[cfg(windows)]
const WINDOWS_TARGETS: &[(&str, &str, &str)] = &[
    ("windows-terminal", "Windows Terminal", "wt.exe"),
    ("powershell", "PowerShell", "powershell.exe"),
];

#[cfg(target_os = "macos")]
fn applescript_string(value: &str) -> String {
    format!("\"{}\"", value.replace('\\', "\\\\").replace('"', "\\\""))
}

#[cfg(target_os = "macos")]
fn shell_single_quoted(value: &str) -> String {
    format!("'{}'", value.replace('\'', "'\\''"))
}

fn argv_for_terminal(id: &str, cmd: &str) -> Vec<String> {
    #[cfg(target_os = "macos")]
    {
        match id {
            "terminal" => vec![
                "-e".into(),
                format!(
                    "tell application \"Terminal\" to do script {}",
                    applescript_string(&format!("cd {}\n{cmd}", shell_single_quoted("~")))
                ),
            ],
            "iterm2" => vec![
                "-e".into(),
                format!(
                    "tell application \"iTerm\" to create window with default profile command {}",
                    applescript_string(cmd)
                ),
            ],
            "warp" => vec![
                "-a".into(),
                "Warp".into(),
                "--".into(),
                "/bin/zsh".into(),
                "-lc".into(),
                cmd.into(),
            ],
            "ghostty" => vec![
                "-a".into(),
                "Ghostty".into(),
                "--".into(),
                "/bin/zsh".into(),
                "-lc".into(),
                cmd.into(),
            ],
            "kitty" => vec!["--".into(), "/bin/zsh".into(), "-lc".into(), cmd.into()],
            "wezterm" => vec![
                "start".into(),
                "--".into(),
                "/bin/zsh".into(),
                "-lc".into(),
                cmd.into(),
            ],
            "alacritty" => vec!["--".into(), "/bin/zsh".into(), "-lc".into(), cmd.into()],
            "vscode" => vec![
                "--new-window".into(),
                "/bin/zsh".into(),
                "-lc".into(),
                cmd.into(),
            ],
            _ => vec![cmd.into()],
        }
    }
    #[cfg(target_os = "linux")]
    {
        match id {
            "konsole" | "xterm" => vec!["-e".into(), "/bin/sh".into(), "-lc".into(), cmd.into()],
            _ => vec!["--".into(), "/bin/sh".into(), "-lc".into(), cmd.into()],
        }
    }
    #[cfg(windows)]
    {
        match id {
            "windows-terminal" => vec![
                "new-tab".into(),
                "--title".into(),
                "TunnelDeck".into(),
                "cmd.exe".into(),
                "/c".into(),
                cmd.into(),
            ],
            _ => vec!["-NoExit".into(), "-Command".into(), cmd.into()],
        }
    }
    #[cfg(not(any(target_os = "macos", target_os = "linux", windows)))]
    {
        let _ = id;
        vec![cmd.into()]
    }
}

fn launcher_command(id: &str) -> &'static str {
    #[cfg(target_os = "macos")]
    {
        match id {
            "terminal" | "iterm2" => "osascript",
            "warp" | "ghostty" => "open",
            "kitty" => "kitty",
            "wezterm" => "wezterm",
            "alacritty" => "alacritty",
            "vscode" => "code",
            _ => "open",
        }
    }
    #[cfg(target_os = "linux")]
    {
        match id {
            "gnome-terminal" => "gnome-terminal",
            "konsole" => "konsole",
            "alacritty" => "alacritty",
            "kitty" => "kitty",
            "xterm" => "xterm",
            _ => "x-terminal-emulator",
        }
    }
    #[cfg(windows)]
    {
        match id {
            "windows-terminal" => "wt.exe",
            "powershell" => "powershell.exe",
            _ => "wt.exe",
        }
    }
    #[cfg(not(any(target_os = "macos", target_os = "linux", windows)))]
    {
        let _ = id;
        "x-terminal-emulator"
    }
}

pub struct OsTerminal;

impl TerminalPort for OsTerminal {
    fn detect(&self) -> Vec<TerminalInfo> {
        #[cfg(target_os = "macos")]
        {
            let mut found = Vec::new();
            for target in MAC_TARGETS {
                let installed = target
                    .bundle_apps
                    .iter()
                    .any(|app| mac_bundle_executable(app).is_some());
                if installed {
                    found.push(TerminalInfo {
                        id: target.id.into(),
                        label: target.label.into(),
                        hint: target.hint.into(),
                        platform: target.platform.into(),
                    });
                }
            }
            found
        }
        #[cfg(target_os = "linux")]
        {
            let platform = "linux";
            LINUX_TARGETS
                .iter()
                .filter(|(id, _, cmd)| {
                    let _ = id;
                    which_on_path(cmd).is_some()
                })
                .map(|(id, label, hint)| TerminalInfo {
                    id: (*id).into(),
                    label: (*label).into(),
                    hint: (*hint).into(),
                    platform: platform.into(),
                })
                .collect()
        }
        #[cfg(windows)]
        {
            let platform = "win32";
            WINDOWS_TARGETS
                .iter()
                .filter(|(_, _, cmd)| which_on_path(cmd).is_some())
                .map(|(id, label, hint)| TerminalInfo {
                    id: (*id).into(),
                    label: (*label).into(),
                    hint: (*hint).into(),
                    platform: platform.into(),
                })
                .collect()
        }
        #[cfg(not(any(target_os = "macos", target_os = "linux", windows)))]
        {
            Vec::new()
        }
    }

    fn launch(&mut self, terminal_id: &str, command_line: &str) -> Result<u32> {
        let known = self.detect().iter().any(|t| t.id == terminal_id);
        if !known {
            return Err(TunnelError::new(
                TunnelProblemCode::NotFound,
                format!("terminal {terminal_id} is not installed on this machine"),
            ));
        }
        let program = launcher_command(terminal_id);
        let args = argv_for_terminal(terminal_id, command_line);
        let mut child = Command::new(program)
            .args(&args)
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn()
            .map_err(|e| {
                TunnelError::new(
                    TunnelProblemCode::TerminalFailed,
                    format!("terminal launcher failed: {e}"),
                )
            })?;
        // Settle window: a launcher that dies instantly is the only observable
        // failure; after that the tunnel lives in the terminal's own tree.
        let deadline = Instant::now() + Duration::from_millis(700);
        loop {
            match child.try_wait() {
                Ok(Some(status)) => {
                    if !status.success() {
                        return Err(TunnelError::new(
                            TunnelProblemCode::TerminalFailed,
                            format!("terminal launcher exited with {status}"),
                        ));
                    }
                    // Exited zero immediately (e.g. `open -a`): the window owns it now.
                    return Ok(child.id());
                }
                Ok(None) => {
                    if Instant::now() >= deadline {
                        return Ok(child.id());
                    }
                    std::thread::sleep(Duration::from_millis(50));
                }
                Err(e) => {
                    return Err(TunnelError::new(
                        TunnelProblemCode::TerminalFailed,
                        format!("terminal launcher failed: {e}"),
                    ));
                }
            }
        }
    }

    fn close(&mut self, pid: u32) {
        kill_pid(pid);
    }
}

// --- open url ------------------------------------------------------------------

pub struct OsOpenUrl;

impl OpenUrlPort for OsOpenUrl {
    fn open(&self, url: &str) -> Result<()> {
        if !(url.starts_with("http://") || url.starts_with("https://")) {
            return Err(TunnelError::new(
                TunnelProblemCode::InvalidRequest,
                "only http and https urls can be opened",
            ));
        }
        #[cfg(target_os = "macos")]
        let program = "open";
        #[cfg(target_os = "linux")]
        let program = "xdg-open";
        #[cfg(windows)]
        let program = "cmd";
        #[cfg(windows)]
        let args: &[&str] = &["/c", "start", "", url];
        #[cfg(not(windows))]
        let args: &[&str] = &[url];
        Command::new(program)
            .args(args)
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn()
            .map_err(|e| {
                TunnelError::new(
                    TunnelProblemCode::UnsupportedOperation,
                    format!("cannot open url: {e}"),
                )
            })?;
        Ok(())
    }
}

// --- ssh config inventory ---------------------------------------------------------

#[derive(Debug)]
struct ConfigBlock {
    patterns: Vec<String>,
    host_name: Option<String>,
    user: Option<String>,
    port: Option<u16>,
}

fn parse_config_text(text: &str) -> Vec<ConfigBlock> {
    let mut blocks = Vec::new();
    let mut patterns: Option<Vec<String>> = None;
    let mut host_name: Option<String> = None;
    let mut user: Option<String> = None;
    let mut port: Option<u16> = None;
    let mut flush = |patterns: &mut Option<Vec<String>>,
                     host_name: &mut Option<String>,
                     user: &mut Option<String>,
                     port: &mut Option<u16>| {
        if let Some(patterns) = patterns.take() {
            blocks.push(ConfigBlock {
                patterns,
                host_name: host_name.take(),
                user: user.take(),
                port: (*port).take(),
            });
        }
    };
    for raw_line in text.lines() {
        let line = raw_line.trim();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        let (key, value) = match line.find(|c: char| c == '=' || c.is_whitespace()) {
            Some(idx) => {
                let (k, rest) = line.split_at(idx);
                (
                    k.trim().to_lowercase(),
                    rest.trim_start_matches(['=', ' ', '\t']).trim().to_string(),
                )
            }
            None => continue,
        };
        if key == "host" {
            flush(&mut patterns, &mut host_name, &mut user, &mut port);
            patterns = Some(value.split_whitespace().map(str::to_string).collect());
            host_name = None;
            user = None;
            port = None;
        } else if patterns.is_some() {
            match key.as_str() {
                "hostname" => {
                    if host_name.is_none() {
                        host_name = Some(value);
                    }
                }
                "user" => {
                    if user.is_none() {
                        user = Some(value);
                    }
                }
                "port" if port.is_none() => {
                    if let Ok(parsed @ 1..=u16::MAX) = value.parse::<u16>() {
                        port = Some(parsed);
                    }
                }
                _ => {}
            }
        }
    }
    flush(&mut patterns, &mut host_name, &mut user, &mut port);
    blocks
}

/// Read-only inventory of the user's own `~/.ssh/config`. Never throws.
pub fn read_ssh_config_hosts() -> Vec<SshHostInfo> {
    let home = std::env::var_os("HOME").or_else(|| std::env::var_os("USERPROFILE"));
    let Some(home) = home else { return Vec::new() };
    read_ssh_config_hosts_in(&PathBuf::from(home))
}

/// Same inventory against an explicit home directory, so tests never touch
/// the user's real config and never mutate the process environment.
pub fn read_ssh_config_hosts_in(home: &Path) -> Vec<SshHostInfo> {
    let path = home.join(".ssh").join("config");
    let Ok(text) = fs::read_to_string(&path) else {
        return Vec::new();
    };
    let blocks = parse_config_text(&text);
    let mut aliases: Vec<String> = Vec::new();
    for block in &blocks {
        for pattern in &block.patterns {
            if pattern.contains(['*', '?']) || aliases.contains(pattern) {
                continue;
            }
            aliases.push(pattern.clone());
        }
    }
    aliases
        .into_iter()
        .map(|alias| {
            let mut host_name = None;
            let mut user = None;
            let mut port = None;
            for block in &blocks {
                if !block.patterns.iter().any(|p| p == &alias) {
                    continue;
                }
                if host_name.is_none() {
                    host_name = block.host_name.clone();
                }
                if user.is_none() {
                    user = block.user.clone();
                }
                if port.is_none() {
                    port = block.port;
                }
            }
            SshHostInfo {
                alias,
                host_name,
                user,
                port,
            }
        })
        .collect()
}

/// Append one line to the tunnel log. Failures are swallowed: logging must
/// never fail the operation it describes.
pub fn append_log_line(log_dir: &Path, id: &str, line: &str) {
    let _ = fs::create_dir_all(log_dir);
    let path = log_dir.join(format!("{id}.log"));
    let mut file = match OpenOptions::new().create(true).append(true).open(&path) {
        Ok(file) => file,
        Err(_) => return,
    };
    let _ = writeln!(file, "{line}");
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_first_value_wins_blocks() {
        let text = "Host ufo\n  HostName 10.0.0.1\n  User hk\n\nHost *\n  User fallback\n";
        let blocks = parse_config_text(text);
        assert_eq!(blocks.len(), 2);
        assert_eq!(blocks[0].patterns, vec!["ufo"]);
        assert_eq!(blocks[0].host_name.as_deref(), Some("10.0.0.1"));
    }

    #[test]
    fn missing_config_yields_no_hosts() {
        // An explicit home dir keeps the test off the user's real config and
        // off the shared process environment.
        let dir = std::env::temp_dir().join(format!("ssh-tunnel-nohome-{}", std::process::id()));
        let _ = fs::create_dir_all(&dir);
        assert!(read_ssh_config_hosts_in(&dir).is_empty());
        let _ = fs::remove_dir_all(&dir);
    }
}
