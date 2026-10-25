//! Tauri commands over the in-process tunnel manager.
//!
//! Each command is a thin adapter: deserialize a typed request, call one
//! `tunnel_core::Manager` method, serialize the typed response. No business
//! logic lives here — the dispatcher shape mirrors
//! `packages/ssh-service/src/http.ts` route for route, so the Web UI can
//! switch transports without relearning the API.
//!
//! Every command is `async` and hands its work to `spawn_blocking`.
//!
//! That is not stylistic: Tauri runs a synchronous command inline on the IPC
//! caller's thread, which on macOS is the main thread — the same thread that
//! paints the WebView. The manager blocks for real (`start` waits out the ssh
//! grace window, `stop` its kill grace), so a sync command froze the window
//! for a second or two and the optimistic "Starting…" state could not be
//! painted until the command returned. Async commands run on the async
//! runtime, and `spawn_blocking` keeps the sleeps off the worker pool.

use serde::{Deserialize, Serialize};
use std::sync::{Arc, Mutex};
use tauri::State;
use tunnel_core::contract::{ForwardRule, LaunchMode, SshHostInfo, SshOption, TunnelMode, TunnelProfile, TunnelState};
use tunnel_core::error::{TunnelError, TunnelProblemCode};
use tunnel_core::host::{
    FileRuntimeStore, FileStore, OsOpenUrl, OsProcess, OsTerminal, SshControl, TcpProbe,
    default_base_dir, read_ssh_config_hosts, supports_control_master,
};
use tunnel_core::ports::SystemClock;
use tunnel_core::manager::{LogTail, Manager, Preview};
use tunnel_core::ports::{ExternalSession, TerminalInfo};

/// The exact manager composition this shell embeds.
pub type AppManager = Manager<
    OsProcess,
    TcpProbe,
    FileStore,
    SystemClock,
    FileRuntimeStore,
    OsTerminal,
    OsOpenUrl,
    SshControl,
>;

pub struct AppState {
    /// Shared so a command can clone the handle into `spawn_blocking`
    /// without holding a borrow across an await.
    pub manager: Arc<Mutex<AppManager>>,
    pub instance_id: String,
}

/// Build the state once at startup. Paths match the daemon layout exactly, so
/// profiles, logs, and runtime records survive the move from sidecar to shell.
pub fn app_state() -> AppState {
    let base = default_base_dir();
    let manager = AppManager::new(
        OsProcess::new("ssh"),
        TcpProbe,
        FileStore::new(base.join("profiles.json")),
        SystemClock,
        FileRuntimeStore::new(base.join("runtime.json")),
        OsTerminal,
        OsOpenUrl,
        supports_control_master().then(|| SshControl::new("ssh")),
        base.join("logs"),
        base.join("control"),
        "ssh".to_string(),
    );
    AppState {
        manager: Arc::new(Mutex::new(manager)),
        instance_id: format!(
            "app-{}-{}",
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map(|d| d.as_millis().to_string())
                .unwrap_or_default(),
            std::process::id()
        ),
    }
}

/// Wire failure, serialized to the Web UI. Consumers branch on `code`, never
/// on `message` — the same rule as the HTTP `{ problem }` envelope.
#[derive(Debug, Serialize)]
pub(crate) struct CommandProblem {
    code: TunnelProblemCode,
    message: String,
    retryable: bool,
}

impl From<TunnelError> for CommandProblem {
    fn from(error: TunnelError) -> Self {
        Self { code: error.code, message: error.message, retryable: error.retryable }
    }
}

fn poisoned() -> CommandProblem {
    CommandProblem {
        code: TunnelProblemCode::InternalError,
        message: "tunnel manager is unavailable".to_string(),
        retryable: false,
    }
}

/// Client-supplied profile fields. The shell stamps `id` + timestamps, exactly
/// like `POST /api/profiles` did.
#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct ProfileInput {
    name: String,
    mode: TunnelMode,
    ssh_target: String,
    #[serde(default)]
    ssh_port: Option<u16>,
    #[serde(default)]
    ssh_user: Option<String>,
    #[serde(default)]
    identity_file: Option<String>,
    #[serde(default)]
    jump_hops: Option<Vec<String>>,
    forwards: Vec<ForwardRule>,
    #[serde(default)]
    extra_options: Option<Vec<SshOption>>,
    #[serde(default)]
    server_alive_interval: Option<u32>,
    #[serde(default)]
    exit_on_forward_failure: Option<bool>,
    #[serde(default)]
    clear_all_forwardings: Option<bool>,
    #[serde(default)]
    no_remote_command: Option<bool>,
    #[serde(default)]
    remote_command: Option<String>,
    #[serde(default)]
    auto_reconnect: Option<bool>,
    #[serde(default)]
    notes: Option<String>,
}

impl ProfileInput {
    fn into_profile(self) -> TunnelProfile {
        TunnelProfile {
            id: String::new(),
            name: self.name,
            mode: self.mode,
            ssh_target: self.ssh_target,
            ssh_port: self.ssh_port,
            ssh_user: self.ssh_user,
            identity_file: self.identity_file,
            jump_hops: self.jump_hops,
            forwards: self.forwards,
            extra_options: self.extra_options,
            server_alive_interval: self.server_alive_interval,
            exit_on_forward_failure: self.exit_on_forward_failure,
            clear_all_forwardings: self.clear_all_forwardings,
            no_remote_command: self.no_remote_command,
            remote_command: self.remote_command,
            auto_reconnect: self.auto_reconnect,
            notes: self.notes,
            created_at: String::new(),
            updated_at: String::new(),
        }
    }
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct Health {
    ok: bool,
    version: String,
    service_instance_id: String,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct OpenUrlResult {
    url: String,
    opened_at: String,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct LaunchResult {
    profile_id: String,
    terminal_id: String,
    mode: LaunchMode,
    command: String,
    pid: u32,
    launched_at: String,
}

impl From<ExternalSession> for LaunchResult {
    fn from(session: ExternalSession) -> Self {
        Self {
            profile_id: session.profile_id,
            terminal_id: session.terminal_id,
            mode: session.mode,
            command: session.command,
            pid: session.pid,
            launched_at: session.launched_at,
        }
    }
}

/// List envelopes mirror the HTTP response shapes exactly, so the shared
/// client validates IPC answers with the same schemas it uses for the daemon.
#[derive(Debug, Serialize)]
pub(crate) struct ProfileList {
    profiles: Vec<TunnelProfile>,
}

#[derive(Debug, Serialize)]
pub(crate) struct StateList {
    states: Vec<TunnelState>,
}

#[derive(Debug, Serialize)]
pub(crate) struct HostList {
    hosts: Vec<SshHostInfo>,
}

#[derive(Debug, Serialize)]
pub(crate) struct TerminalList {
    terminals: Vec<TerminalInfo>,
}

/// Run one manager call off the IPC thread.
///
/// The closure gets `&mut AppManager`; the handle is cloned first so nothing
/// borrowed crosses the await. A poisoned mutex and a panicking worker both
/// surface as `InternalError`, never as a silent hang.
async fn run<T, F>(state: &AppState, work: F) -> Result<T, CommandProblem>
where
    T: Send + 'static,
    F: FnOnce(&mut AppManager) -> Result<T, TunnelError> + Send + 'static,
{
    let manager = Arc::clone(&state.manager);
    tauri::async_runtime::spawn_blocking(move || {
        let mut guard = manager.lock().map_err(|_| poisoned())?;
        work(&mut guard).map_err(CommandProblem::from)
    })
    .await
    .map_err(|_| poisoned())?
}

#[tauri::command]
pub async fn tunnel_health(state: State<'_, AppState>) -> Result<Health, CommandProblem> {
    // Nothing blocks here, but returning the same `Result` shape keeps every
    // command uniform for the client (`invoke` rejects on Err).
    Ok(Health {
        ok: true,
        version: env!("CARGO_PKG_VERSION").to_string(),
        service_instance_id: state.instance_id.clone(),
    })
}

#[tauri::command]
pub async fn tunnel_list_profiles(state: State<'_, AppState>) -> Result<ProfileList, CommandProblem> {
    run(&state, |manager| {
        Ok(ProfileList { profiles: manager.list_profiles()? })
    })
    .await
}

#[tauri::command]
pub async fn tunnel_create_profile(
    state: State<'_, AppState>,
    input: ProfileInput,
) -> Result<TunnelProfile, CommandProblem> {
    run(&state, move |manager| manager.create_profile(input.into_profile())).await
}

#[tauri::command]
pub async fn tunnel_get_profile(
    state: State<'_, AppState>,
    id: String,
) -> Result<TunnelProfile, CommandProblem> {
    run(&state, move |manager| manager.get_profile(&id)).await
}

#[tauri::command]
pub async fn tunnel_update_profile(
    state: State<'_, AppState>,
    id: String,
    input: ProfileInput,
) -> Result<TunnelProfile, CommandProblem> {
    run(&state, move |manager| manager.update_profile(&id, input.into_profile())).await
}

#[tauri::command]
pub async fn tunnel_delete_profile(
    state: State<'_, AppState>,
    id: String,
) -> Result<(), CommandProblem> {
    run(&state, move |manager| manager.delete_profile(&id)).await
}

#[tauri::command]
pub async fn tunnel_list_states(state: State<'_, AppState>) -> Result<StateList, CommandProblem> {
    // The poll path: `list_states` runs `ssh -O check` per profile, which is
    // real process I/O. On the IPC thread it would stutter the window every
    // three seconds; here it only occupies a blocked worker.
    run(&state, |manager| Ok(StateList { states: manager.list_states()? })).await
}

#[tauri::command]
pub async fn tunnel_get_state(
    state: State<'_, AppState>,
    id: String,
) -> Result<TunnelState, CommandProblem> {
    run(&state, move |manager| manager.get_state(&id)).await
}

#[tauri::command]
pub async fn tunnel_preview(
    state: State<'_, AppState>,
    id: String,
) -> Result<PreviewWire, CommandProblem> {
    run(&state, move |manager| {
        let preview: Preview = manager.preview(&id)?;
        Ok(PreviewWire {
            profile_id: id,
            argv: preview.argv,
            command: preview.command,
        })
    })
    .await
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct PreviewWire {
    profile_id: String,
    argv: Vec<String>,
    command: String,
}

#[tauri::command]
pub async fn tunnel_start(
    state: State<'_, AppState>,
    id: String,
) -> Result<TunnelState, CommandProblem> {
    // Waits out the ssh grace window (0.9s+, 4s per jump hop). Off-thread is
    // what lets the page paint "Starting…" before this returns.
    run(&state, move |manager| manager.start(&id)).await
}

#[tauri::command]
pub async fn tunnel_stop(
    state: State<'_, AppState>,
    id: String,
) -> Result<TunnelState, CommandProblem> {
    // Kill grace: 1.2s, then up to 0.4s more. Same reason as `start`.
    run(&state, move |manager| manager.stop(&id)).await
}

#[tauri::command]
pub async fn tunnel_stop_all(state: State<'_, AppState>) -> Result<StateList, CommandProblem> {
    run(&state, |manager| Ok(StateList { states: manager.stop_all() })).await
}

#[tauri::command]
pub async fn tunnel_probe(
    state: State<'_, AppState>,
    id: String,
) -> Result<TunnelState, CommandProblem> {
    // Up to three dials at 2.5s each plus two 800ms waits.
    run(&state, move |manager| manager.probe_tunnel(&id)).await
}

#[tauri::command]
pub async fn tunnel_log_tail(
    state: State<'_, AppState>,
    id: String,
    tail: Option<usize>,
) -> Result<LogTailWire, CommandProblem> {
    run(&state, move |manager| {
        let log: LogTail = manager.log_tail(&id, tail.unwrap_or(200).clamp(1, 2000))?;
        Ok(LogTailWire {
            profile_id: log.profile_id,
            path: log.path.to_string_lossy().into_owned(),
            lines: log.lines,
            truncated: log.truncated,
            bytes: log.bytes,
        })
    })
    .await
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct LogTailWire {
    profile_id: String,
    path: String,
    lines: Vec<String>,
    truncated: bool,
    bytes: u64,
}

#[tauri::command]
pub async fn tunnel_list_hosts() -> Result<HostList, CommandProblem> {
    // Reads and parses ~/.ssh/config: file I/O, so off the IPC thread too.
    tauri::async_runtime::spawn_blocking(|| HostList { hosts: read_ssh_config_hosts() })
        .await
        .map_err(|_| poisoned())
}

#[tauri::command]
pub async fn tunnel_list_terminals(
    state: State<'_, AppState>,
) -> Result<TerminalList, CommandProblem> {
    // Probes /Applications for app bundles.
    run(&state, |manager| Ok(TerminalList { terminals: manager.list_terminals() })).await
}

#[tauri::command]
pub async fn tunnel_launch_in_terminal(
    state: State<'_, AppState>,
    id: String,
    terminal_id: String,
    mode: Option<LaunchMode>,
) -> Result<LaunchResult, CommandProblem> {
    // Waits out the launcher settle window (700ms).
    run(&state, move |manager| {
        manager
            .launch_in_terminal(&id, &terminal_id, mode.unwrap_or(LaunchMode::Tunnel))
            .map(LaunchResult::from)
    })
    .await
}

#[tauri::command]
pub async fn tunnel_close_session(
    state: State<'_, AppState>,
    id: String,
) -> Result<bool, CommandProblem> {
    run(&state, move |manager| Ok(manager.close_session(&id))).await
}

#[tauri::command]
pub async fn tunnel_open_url(
    state: State<'_, AppState>,
    url: String,
) -> Result<OpenUrlResult, CommandProblem> {
    run(&state, move |manager| {
        let opened_at = manager.open_url(&url)?;
        Ok(OpenUrlResult { url, opened_at })
    })
    .await
}
