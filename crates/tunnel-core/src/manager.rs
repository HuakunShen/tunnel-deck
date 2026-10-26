#![forbid(unsafe_code)]
//! Tunnel manager — the stateful core. Owns profiles (persisted via
//! `StorePort`), live `ssh` children (via `ProcessPort`), and per-tunnel
//! runtime states. All transitions go through the state machine so every
//! front observes identical rules. Mirrors
//! `packages/ssh-service/src/manager.ts`; blocking by design — the Tauri
//! shell calls it from `spawn_blocking`, and tests call it directly.

use crate::argv::{build_ssh_argv, local_bind_ports, ssh_destination, ArgvOptions};
use crate::contract::{
    ForwardHealth, ForwardKind, ForwardRule, LaunchMode, TunnelOrigin, TunnelProfile, TunnelState,
    TunnelStatus,
};
use crate::error::{Result, TunnelError, TunnelProblemCode};
use crate::host::append_log_line;
use crate::ports::{
    ClockPort, ControlPort, ExternalSession, LiveChild, OpenUrlPort, ProbePort, ProcessPort,
    RuntimeOrigin, RuntimeRecord, RuntimeStorePort, StorePort, TerminalPort,
};
use crate::state_machine::{can_transition, start_target, stop_target};
use crate::validate::{validate_profile, Severity};
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::time::Duration;

/// How long a freshly handed-off window gets before its control socket must
/// answer. ssh needs a moment to authenticate and bind the master.
const TERMINAL_START_GRACE: Duration = Duration::from_millis(12_000);

const MAX_PROFILES: usize = 200;
const MAX_LOG_WINDOW: u64 = 128 * 1024;

#[derive(Debug)]
pub struct LogTail {
    pub profile_id: String,
    pub path: PathBuf,
    pub lines: Vec<String>,
    pub truncated: bool,
    pub bytes: u64,
}

#[derive(Debug)]
pub struct Preview {
    pub argv: Vec<String>,
    pub command: String,
}

struct LiveTunnel {
    child: LiveChild,
    argv: Vec<String>,
}

fn new_id(clock: &dyn ClockPort) -> String {
    let now = clock.now_iso();
    let suffix: String = now
        .chars()
        .filter(|c| c.is_ascii_alphanumeric())
        .take(6)
        .collect::<String>()
        .to_lowercase();
    // Millisecond time in base36 keeps ids sortable and unique per process.
    let millis = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_millis())
        .unwrap_or(0);
    let mut digits = Vec::new();
    let mut value = millis;
    if value == 0 {
        digits.push('0');
    }
    while value > 0 {
        let digit = (value % 36) as u8;
        digits.push((b"0123456789abcdefghijklmnopqrstuvwxyz")[digit as usize] as char);
        value /= 36;
    }
    digits.reverse();
    let time: String = digits.into_iter().collect();
    format!("t-{time}-{suffix}")
}

/// Reads the tail of a failed startup log and names the likely cause.
/// A `No route to host` for a LAN address while the same command works in a
/// terminal is the Local Network privacy grant, not the network: the grant is
/// per-app identity, Terminal has it, and an ad-hoc-signed app may not (or
/// the grant reset on rebuild). Returns `None` when the log says nothing
/// recognizable, so unrelated failures keep their plain message.
fn startup_failure_hint(log_file: &std::path::Path) -> Option<String> {
    let raw = std::fs::read(log_file).ok()?;
    let tail = &raw[raw.len().saturating_sub(4096)..];
    let text = String::from_utf8_lossy(tail);
    if !text.contains("No route to host") {
        return None;
    }
    let lan = text.split_whitespace().any(|word| {
        let ip = word.trim_matches(|c: char| !c.is_ascii_alphanumeric() && c != '.');
        ip.starts_with("192.168.")
            || ip.starts_with("10.")
            || (ip.starts_with("172.")
                && ip
                    .split('.')
                    .nth(1)
                    .and_then(|octet| octet.parse::<u8>().ok())
                    .is_some_and(|octet| (16..=31).contains(&octet)))
    });
    if !lan {
        return None;
    }
    Some(
        "hint: this host is on your LAN and the same command works in a terminal, \
         so macOS is likely blocking this app's local-network access. Enable it under \
         System Settings → Privacy & Security → Local Network → TunnelDeck, then start again."
            .to_string(),
    )
}

fn health_for_probe(
    profile: &TunnelProfile,
    results: &HashMap<u16, (bool, Option<u64>, Option<String>)>,
) -> Vec<ForwardHealth> {
    profile
        .forwards
        .iter()
        .map(|rule| match rule {
            ForwardRule::Remote { bind_port, .. } => ForwardHealth {
                kind: ForwardKind::Remote,
                bind_port: *bind_port,
                ok: true,
                latency_ms: None,
                detail: Some(
                    "remote forwards listen on the server; verified via ssh exit status".into(),
                ),
            },
            rule => {
                let port = rule.bind_port();
                let kind = rule.kind();
                match results.get(&port) {
                    None => ForwardHealth {
                        kind,
                        bind_port: port,
                        ok: false,
                        latency_ms: None,
                        detail: Some("not probed yet".into()),
                    },
                    Some((ok, latency_ms, detail)) => ForwardHealth {
                        kind,
                        bind_port: port,
                        ok: *ok,
                        latency_ms: *latency_ms,
                        detail: detail.clone(),
                    },
                }
            }
        })
        .collect()
}

fn validate_for_save(profile: &TunnelProfile) -> Result<()> {
    // Schema-level guards that zod enforces on the TS side.
    if profile.name.len() > 120 || profile.ssh_target.len() > 512 || profile.forwards.is_empty() {
        return Err(TunnelError::new(
            TunnelProblemCode::InvalidRequest,
            "profile input is malformed",
        ));
    }
    let findings = validate_profile(profile);
    if let Some(blocking) = findings.iter().find(|f| f.severity == Severity::Error) {
        return Err(TunnelError::new(
            TunnelProblemCode::InvalidRequest,
            blocking.message.clone(),
        ));
    }
    Ok(())
}

pub struct Manager<P, Q, S, C, R, T, O, K>
where
    P: ProcessPort,
    Q: ProbePort,
    S: StorePort,
    C: ClockPort,
    R: RuntimeStorePort,
    T: TerminalPort,
    O: OpenUrlPort,
    K: ControlPort,
{
    process: P,
    prober: Q,
    store: S,
    clock: C,
    runtime: R,
    terminal: T,
    open_url: O,
    control: Option<K>,
    multiplexed: bool,
    log_dir: PathBuf,
    control_dir: PathBuf,
    ssh_path: String,
    profiles: HashMap<String, TunnelProfile>,
    states: HashMap<String, TunnelState>,
    live: HashMap<String, LiveTunnel>,
    sessions: HashMap<String, ExternalSession>,
    loaded: bool,
}

impl<P, Q, S, C, R, T, O, K> Manager<P, Q, S, C, R, T, O, K>
where
    P: ProcessPort,
    Q: ProbePort,
    S: StorePort,
    C: ClockPort,
    R: RuntimeStorePort,
    T: TerminalPort,
    O: OpenUrlPort,
    K: ControlPort,
{
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        process: P,
        prober: Q,
        store: S,
        clock: C,
        runtime: R,
        terminal: T,
        open_url: O,
        control: Option<K>,
        log_dir: PathBuf,
        control_dir: PathBuf,
        ssh_path: String,
    ) -> Self {
        let multiplexed = control.is_some();
        Self {
            process,
            prober,
            store,
            clock,
            runtime,
            terminal,
            open_url,
            control,
            multiplexed,
            log_dir,
            control_dir,
            ssh_path,
            profiles: HashMap::new(),
            states: HashMap::new(),
            live: HashMap::new(),
            sessions: HashMap::new(),
            loaded: false,
        }
    }

    fn ensure_loaded(&mut self) -> Result<()> {
        if self.loaded {
            return Ok(());
        }
        let profiles = self.store.read_profiles()?;
        for profile in profiles {
            self.states.insert(
                profile.id.clone(),
                TunnelState {
                    profile_id: profile.id.clone(),
                    status: TunnelStatus::Stopped,
                    origin: None,
                    pid: None,
                    started_at: None,
                    last_error: None,
                    forwards: Vec::new(),
                    argv: None,
                },
            );
            self.profiles.insert(profile.id.clone(), profile);
        }
        self.loaded = true;
        self.adopt_runtime();
        Ok(())
    }

    fn adopt_runtime(&mut self) {
        let records = self.runtime.read();
        if records.is_empty() {
            return;
        }
        let mut keep = Vec::new();
        for record in records {
            let Some(profile) = self.profiles.get(&record.profile_id) else {
                continue;
            };
            if !self.runtime_alive(&record, profile) {
                continue;
            }
            keep.push(record.clone());
            let origin = match record.origin {
                RuntimeOrigin::Managed => TunnelOrigin::Managed,
                RuntimeOrigin::Terminal => TunnelOrigin::Terminal,
            };
            self.states.insert(
                record.profile_id.clone(),
                TunnelState {
                    profile_id: record.profile_id.clone(),
                    status: TunnelStatus::Running,
                    origin: Some(origin),
                    pid: record.pid,
                    started_at: Some(record.started_at.clone()),
                    last_error: None,
                    forwards: Vec::new(),
                    argv: None,
                },
            );
            if record.origin == RuntimeOrigin::Terminal {
                let mode = record.mode.unwrap_or(LaunchMode::Tunnel);
                let control_path = self.handoff_control_path(&record.profile_id);
                let command = build_ssh_argv(
                    profile,
                    ArgvOptions {
                        control_path: control_path.as_deref(),
                        interactive: mode == LaunchMode::Shell,
                    },
                )
                .command;
                self.sessions.insert(
                    record.profile_id.clone(),
                    ExternalSession {
                        profile_id: record.profile_id.clone(),
                        terminal_id: record
                            .terminal_id
                            .clone()
                            .unwrap_or_else(|| "terminal".into()),
                        mode,
                        command,
                        pid: record.pid.unwrap_or(0),
                        launched_at: record.started_at.clone(),
                    },
                );
            }
        }
        self.runtime.write(&keep);
    }

    fn persist_runtime(&self) {
        let mut records = Vec::new();
        for (id, state) in &self.states {
            let origin = match state.origin {
                Some(TunnelOrigin::Managed) => RuntimeOrigin::Managed,
                Some(TunnelOrigin::Terminal) => RuntimeOrigin::Terminal,
                _ => continue,
            };
            if !matches!(
                state.status,
                TunnelStatus::Running | TunnelStatus::Degraded | TunnelStatus::Starting
            ) {
                continue;
            }
            let session = self.sessions.get(id);
            records.push(RuntimeRecord {
                profile_id: id.clone(),
                origin,
                started_at: state
                    .started_at
                    .clone()
                    .unwrap_or_else(|| self.clock.now_iso()),
                pid: state.pid,
                mode: session.map(|s| s.mode),
                terminal_id: session.map(|s| s.terminal_id.clone()),
            });
        }
        self.runtime.write(&records);
    }

    fn persist(&self) -> Result<()> {
        let mut profiles: Vec<TunnelProfile> = self.profiles.values().cloned().collect();
        profiles.sort_by(|a, b| a.name.cmp(&b.name));
        self.store.write_profiles(&profiles)
    }

    fn set_status(
        &mut self,
        id: &str,
        to: TunnelStatus,
        patch: impl FnOnce(&mut TunnelState),
    ) -> Result<()> {
        let Some(state) = self.states.get_mut(id) else {
            return Err(TunnelError::new(
                TunnelProblemCode::NotFound,
                format!("no tunnel {id}"),
            ));
        };
        if !can_transition(state.status, to) {
            return Err(TunnelError::new(
                TunnelProblemCode::Conflict,
                format!("cannot move tunnel {id} from {:?} to {to:?}", state.status),
            ));
        }
        state.status = to;
        patch(state);
        Ok(())
    }

    fn control_path_for(&self, id: &str) -> PathBuf {
        self.control_dir.join(format!("{id}.sock"))
    }

    fn handoff_control_path(&self, id: &str) -> Option<String> {
        if self.multiplexed {
            Some(self.control_path_for(id).to_string_lossy().into_owned())
        } else {
            None
        }
    }

    /// Reclaim this profile's control-socket path before a new master binds it.
    ///
    /// A dead master leaves its socket file behind; a new ssh would then warn
    /// "already exists, disabling multiplexing" and run without one —
    /// unstoppable and unadoptable afterwards. Remove the file when nothing
    /// answers behind it; refuse when something does, so a live session (our
    /// window or the user's own) is never stolen.
    fn reclaim_socket(&self, id: &str, profile: &TunnelProfile) -> Result<()> {
        if !self.multiplexed {
            return Ok(());
        }
        let _ = std::fs::create_dir_all(&self.control_dir);
        let socket = self.control_path_for(id);
        if !socket.exists() {
            return Ok(());
        }
        if self.socket_alive(id, profile) {
            return Err(TunnelError::new(
                TunnelProblemCode::Conflict,
                format!("tunnel {id} already has a live session on its control socket"),
            ));
        }
        let _ = std::fs::remove_file(&socket);
        Ok(())
    }

    fn socket_alive(&self, id: &str, profile: &TunnelProfile) -> bool {
        let Some(control) = &self.control else {
            return false;
        };
        let path = self.control_path_for(id);
        if !path.exists() {
            return false;
        }
        control.check(&path, &ssh_destination(profile))
    }

    fn runtime_alive(&self, record: &RuntimeRecord, profile: &TunnelProfile) -> bool {
        if self.socket_alive(&record.profile_id, profile) {
            return true;
        }
        if self.multiplexed {
            return false;
        }
        record.origin == RuntimeOrigin::Managed
            && record.pid.is_some_and(|pid| self.process.is_alive(pid))
    }

    fn bind_port_answers(&self, profile: &TunnelProfile) -> bool {
        for rule in &profile.forwards {
            let (bind_address, bind_port) = match rule {
                ForwardRule::Remote { .. } => continue,
                ForwardRule::Local {
                    bind_address,
                    bind_port,
                    ..
                }
                | ForwardRule::Dynamic {
                    bind_address,
                    bind_port,
                } => (bind_address, *bind_port),
            };
            let host = match bind_address.as_deref().unwrap_or("") {
                "" | "*" | "0.0.0.0" => "127.0.0.1",
                other => other,
            };
            if self.prober.dial_tcp(host, bind_port, 400).ok {
                return true;
            }
        }
        false
    }

    fn refresh_unmanaged(&mut self) {
        let ids: Vec<String> = self.states.keys().cloned().collect();
        for id in ids {
            let Some(profile) = self.profiles.get(&id).cloned() else {
                continue;
            };
            let Some(state) = self.states.get(&id).cloned() else {
                continue;
            };
            if let Some(session) = self.sessions.get(&id).cloned() {
                let alive = self.control.as_ref().is_some_and(|control| {
                    control.check(&self.control_path_for(&id), &ssh_destination(&profile))
                });
                if alive {
                    self.states.insert(
                        id.clone(),
                        TunnelState {
                            status: TunnelStatus::Running,
                            origin: Some(TunnelOrigin::Terminal),
                            last_error: None,
                            ..state
                        },
                    );
                    continue;
                }
                // Grace window for a window that is still connecting: a
                // session launched by this process stays `starting` until the
                // socket answers. Adopted sessions (from the runtime file)
                // predate the process, so the grace window never applies.
                if self.session_is_fresh(&session) {
                    self.states.insert(
                        id.clone(),
                        TunnelState {
                            status: TunnelStatus::Starting,
                            origin: Some(TunnelOrigin::Terminal),
                            ..state
                        },
                    );
                    continue;
                }
                self.sessions.remove(&id);
                let still_listening = self.bind_port_answers(&profile);
                self.persist_runtime();
                self.states.insert(
                    id.clone(),
                    if still_listening {
                        TunnelState {
                            status: TunnelStatus::Running,
                            origin: Some(TunnelOrigin::External),
                            last_error: Some(
                                "the terminal session ended; a listener still holds this port"
                                    .into(),
                            ),
                            ..state
                        }
                    } else {
                        TunnelState {
                            profile_id: id.clone(),
                            status: TunnelStatus::Stopped,
                            origin: Some(TunnelOrigin::Terminal),
                            pid: None,
                            started_at: None,
                            last_error: None,
                            forwards: Vec::new(),
                            argv: None,
                        }
                    },
                );
                continue;
            }
            if let Some(live) = self.live.get(&id) {
                if self.process.is_alive(live.child.pid) {
                    continue;
                }
            }
            if state.origin == Some(TunnelOrigin::Managed) && !self.live.contains_key(&id) {
                if self.socket_alive(&id, &profile) {
                    continue;
                }
                if !self.multiplexed && state.pid.is_some_and(|pid| self.process.is_alive(pid)) {
                    continue;
                }
                let mut next = state.clone();
                next.status = TunnelStatus::Stopped;
                next.pid = None;
                next.started_at = None;
                next.forwards = Vec::new();
                next.last_error = Some("the tunnel exited while the host was not running".into());
                self.states.insert(id.clone(), next);
                self.persist_runtime();
                continue;
            }
            if self.socket_alive(&id, &profile) {
                let mut next = state.clone();
                next.status = TunnelStatus::Running;
                next.origin = Some(TunnelOrigin::Terminal);
                next.last_error = None;
                self.states.insert(id.clone(), next);
                continue;
            }
            if state.origin == Some(TunnelOrigin::External) {
                if self.bind_port_answers(&profile) {
                    continue;
                }
                let mut next = state.clone();
                next.status = TunnelStatus::Stopped;
                next.pid = None;
                next.started_at = None;
                next.forwards = Vec::new();
                self.states.insert(id.clone(), next);
                continue;
            }
            if state.status != TunnelStatus::Stopped {
                continue;
            }
            if self.bind_port_answers(&profile) {
                self.states.insert(
                    id.clone(),
                    TunnelState {
                        profile_id: id.clone(),
                        status: TunnelStatus::Running,
                        origin: Some(TunnelOrigin::External),
                        pid: None,
                        started_at: None,
                        last_error: Some(
                            "started outside this host; state comes from the bind port".into(),
                        ),
                        forwards: Vec::new(),
                        argv: None,
                    },
                );
            }
        }
    }

    /// Whether a terminal session is still inside its start grace window.
    /// ISO8601 UTC strings compare lexicographically, so `launched_at` orders
    /// against `now` without parsing. An unparseable stamp gets no grace —
    /// fail closed toward reaping, never toward claiming forever.
    fn session_is_fresh(&self, session: &ExternalSession) -> bool {
        let Ok(launched) = chrono::DateTime::parse_from_rfc3339(&session.launched_at) else {
            return false;
        };
        let Ok(now) = chrono::DateTime::parse_from_rfc3339(&self.clock.now_iso()) else {
            return false;
        };
        now.signed_duration_since(launched)
            < chrono::Duration::from_std(TERMINAL_START_GRACE).unwrap_or(chrono::Duration::zero())
    }

    fn refresh_state(&mut self, id: &str) -> Result<TunnelState> {
        let Some(state) = self.states.get(id).cloned() else {
            return Err(TunnelError::new(
                TunnelProblemCode::NotFound,
                format!("no tunnel {id}"),
            ));
        };
        if self.sessions.contains_key(id)
            || state.origin == Some(TunnelOrigin::Terminal)
            || state.origin == Some(TunnelOrigin::External)
        {
            return Ok(state);
        }
        if state.origin == Some(TunnelOrigin::Managed) && !self.live.contains_key(id) {
            return Ok(state);
        }
        let Some(live) = self.live.get(id) else {
            return Ok(state);
        };
        if matches!(state.status, TunnelStatus::Running | TunnelStatus::Degraded)
            && !self.process.is_alive(live.child.pid)
        {
            let mut failed = state.clone();
            failed.status = TunnelStatus::Failed;
            failed.last_error =
                Some("ssh process exited unexpectedly; see log for stderr tail".into());
            self.states.insert(id.into(), failed.clone());
            self.live.remove(id);
            return Ok(failed);
        }
        Ok(state)
    }

    // -- profiles ----------------------------------------------------------

    pub fn list_profiles(&mut self) -> Result<Vec<TunnelProfile>> {
        self.ensure_loaded()?;
        let mut profiles: Vec<TunnelProfile> = self.profiles.values().cloned().collect();
        profiles.sort_by(|a, b| a.name.cmp(&b.name));
        Ok(profiles)
    }

    pub fn get_profile(&mut self, id: &str) -> Result<TunnelProfile> {
        self.ensure_loaded()?;
        self.profiles.get(id).cloned().ok_or_else(|| {
            TunnelError::new(TunnelProblemCode::NotFound, format!("no profile {id}"))
        })
    }

    pub fn create_profile(&mut self, mut input: TunnelProfile) -> Result<TunnelProfile> {
        self.ensure_loaded()?;
        if self.profiles.len() >= MAX_PROFILES {
            return Err(TunnelError::new(
                TunnelProblemCode::LimitExceeded,
                format!("at most {MAX_PROFILES} profiles"),
            ));
        }
        validate_for_save(&input)?;
        let now = self.clock.now_iso();
        input.id = new_id(&self.clock);
        input.created_at = now.clone();
        input.updated_at = now;
        self.states.insert(
            input.id.clone(),
            TunnelState {
                profile_id: input.id.clone(),
                status: TunnelStatus::Stopped,
                origin: None,
                pid: None,
                started_at: None,
                last_error: None,
                forwards: Vec::new(),
                argv: None,
            },
        );
        self.profiles.insert(input.id.clone(), input.clone());
        self.persist()?;
        Ok(input)
    }

    pub fn update_profile(&mut self, id: &str, mut input: TunnelProfile) -> Result<TunnelProfile> {
        self.ensure_loaded()?;
        let existing = self.get_profile(id)?;
        if let Some(state) = self.states.get(id) {
            if state.status != TunnelStatus::Stopped && state.status != TunnelStatus::Failed {
                return Err(TunnelError::new(
                    TunnelProblemCode::Conflict,
                    format!("stop tunnel {id} before editing it"),
                ));
            }
        }
        validate_for_save(&input)?;
        input.id = id.into();
        input.created_at = existing.created_at;
        input.updated_at = self.clock.now_iso();
        self.profiles.insert(id.into(), input.clone());
        self.persist()?;
        Ok(input)
    }

    pub fn delete_profile(&mut self, id: &str) -> Result<()> {
        self.ensure_loaded()?;
        self.get_profile(id)?;
        if let Some(state) = self.states.get(id) {
            if state.status != TunnelStatus::Stopped && state.status != TunnelStatus::Failed {
                return Err(TunnelError::new(
                    TunnelProblemCode::Conflict,
                    format!("stop tunnel {id} before deleting it"),
                ));
            }
        }
        self.close_session(id);
        self.profiles.remove(id);
        self.states.remove(id);
        self.persist()?;
        self.persist_runtime();
        Ok(())
    }

    // -- runtime -----------------------------------------------------------

    pub fn list_states(&mut self) -> Result<Vec<TunnelState>> {
        self.ensure_loaded()?;
        self.refresh_unmanaged();
        Ok(self.states.values().cloned().collect())
    }

    pub fn get_state(&mut self, id: &str) -> Result<TunnelState> {
        self.ensure_loaded()?;
        self.refresh_unmanaged();
        self.refresh_state(id)
    }

    pub fn preview(&mut self, id: &str) -> Result<Preview> {
        self.ensure_loaded()?;
        let profile = self.get_profile(id)?;
        let built = build_ssh_argv(&profile, ArgvOptions::default());
        Ok(Preview {
            argv: built.argv,
            command: built.command,
        })
    }

    pub fn start(&mut self, id: &str) -> Result<TunnelState> {
        self.ensure_loaded()?;
        let profile = self.get_profile(id)?;
        let current = self.refresh_state(id)?;
        let Some(_) = start_target(current.status) else {
            return Err(TunnelError::new(
                TunnelProblemCode::Conflict,
                format!(
                    "tunnel {id} is {:?}; stop it before starting",
                    current.status
                ),
            ));
        };
        self.set_status(id, TunnelStatus::Starting, |s| {
            s.last_error = None;
        })?;

        self.reclaim_socket(id, &profile)?;
        append_log_line(
            &self.log_dir,
            id,
            &format!("--- starting {} ---", self.clock.now_iso()),
        );
        // Fail fast with a human message when the bind port is already held —
        // by an orphan ssh, a second app instance, or an unrelated service.
        // Without this, ssh dies on `Address already in use` mid-startup and
        // the failure reads as a connection problem, which it is not.
        for rule in &profile.forwards {
            let (bind_address, bind_port) = match rule {
                ForwardRule::Remote { .. } => continue,
                ForwardRule::Local {
                    bind_address,
                    bind_port,
                    ..
                }
                | ForwardRule::Dynamic {
                    bind_address,
                    bind_port,
                } => (bind_address, *bind_port),
            };
            let host = match bind_address.as_deref().unwrap_or("") {
                "" | "*" | "0.0.0.0" => "127.0.0.1",
                other => other,
            };
            if self.prober.port_held(host, bind_port) {
                self.set_status(id, TunnelStatus::Failed, |s| {
                    s.last_error = Some(format!(
                        "local port {bind_port} is already in use — another tunnel, a leftover ssh, or another app may hold it; free it and start again"
                    ));
                })?;
                return self.states.get(id).cloned().ok_or_else(|| {
                    TunnelError::new(TunnelProblemCode::InternalError, "state lost during start")
                });
            }
        }
        let control_path = self.handoff_control_path(id);
        let built = build_ssh_argv(
            &profile,
            ArgvOptions {
                control_path: control_path.as_deref(),
                interactive: false,
            },
        );
        let _ = std::fs::create_dir_all(&self.log_dir);
        let log_file = self.log_dir.join(format!("{id}.log"));
        let child = match self.process.spawn(&built.argv, &log_file) {
            Ok(child) => child,
            Err(e) => {
                self.set_status(id, TunnelStatus::Failed, |s| {
                    s.last_error = Some(e.message.clone());
                })?;
                return self.states.get(id).cloned().ok_or_else(|| {
                    TunnelError::new(TunnelProblemCode::InternalError, "state lost during start")
                });
            }
        };
        if child.pid == 0 {
            self.set_status(id, TunnelStatus::Failed, |s| {
                s.last_error = Some("ssh executable not found".into());
            })?;
            return self.states.get(id).cloned().ok_or_else(|| {
                TunnelError::new(TunnelProblemCode::InternalError, "state lost during start")
            });
        }

        let started_at = self.clock.now_iso();
        // Poll the child handle (not just the pid table) so a fast failure is
        // observed even where `is_alive` lags.
        self.live.insert(
            id.into(),
            LiveTunnel {
                child,
                argv: built.argv,
            },
        );

        let hop_count = profile.jump_hops.as_ref().map_or(0, Vec::len) as u64;
        let grace = Duration::from_millis(900 + hop_count * 4000);
        std::thread::sleep(grace);
        // Reap a child that exited during the grace window.
        let exited_early = self
            .live
            .get_mut(id)
            .and_then(|live| live.child.child.as_mut())
            .and_then(|child| child.try_wait().ok().flatten())
            .is_some();
        let alive = self
            .live
            .get(id)
            .is_some_and(|live| self.process.is_alive(live.child.pid))
            && !exited_early;
        if !alive {
            self.live.remove(id);
            // A master that died mid-startup may still have left its socket
            // file; clear it while nothing answers behind it.
            if self.multiplexed {
                let socket = self.control_path_for(id);
                if !self.socket_alive(id, &profile) {
                    let _ = std::fs::remove_file(&socket);
                }
            }
            let needs_fail = self
                .states
                .get(id)
                .is_some_and(|s| s.status == TunnelStatus::Starting);
            if needs_fail {
                // Surface the cause, not just the log path: a LAN host that is
                // unreachable from the app while the same command works in a
                // terminal is almost always the system's Local Network privacy
                // grant (per-app identity; Terminal has it, this app may not).
                let cause = startup_failure_hint(&log_file);
                self.set_status(id, TunnelStatus::Failed, |s| {
                    s.last_error = Some(format!(
                        "ssh exited during startup; see {}{}",
                        log_file.display(),
                        cause.map(|hint| format!("\n{hint}")).unwrap_or_default()
                    ));
                })?;
            }
            append_log_line(&self.log_dir, id, "ssh exited during startup");
            return self.states.get(id).cloned().ok_or_else(|| {
                TunnelError::new(TunnelProblemCode::InternalError, "state lost during start")
            });
        }
        let pid = self.live.get(id).map(|live| live.child.pid).unwrap_or(0);
        let argv = self
            .live
            .get(id)
            .map(|live| live.argv.clone())
            .unwrap_or_default();
        self.set_status(id, TunnelStatus::Running, |s| {
            s.origin = Some(TunnelOrigin::Managed);
            s.pid = Some(pid);
            s.started_at = Some(started_at.clone());
            s.argv = Some(argv.clone());
        })?;
        self.persist_runtime();
        match self.probe_tunnel(id) {
            Ok(probed) => Ok(probed),
            Err(_) => self.states.get(id).cloned().ok_or_else(|| {
                TunnelError::new(TunnelProblemCode::InternalError, "state lost during start")
            }),
        }
    }

    pub fn stop(&mut self, id: &str) -> Result<TunnelState> {
        self.ensure_loaded()?;
        let refreshed = self.refresh_state(id)?;
        let live_exists = self.live.contains_key(id);
        let profile = self.get_profile(id)?;
        let socket_owned = !live_exists && self.socket_alive(id, &profile);
        if start_target(refreshed.status).is_none()
            && stop_target(refreshed.status).is_none()
            && !matches!(
                refreshed.status,
                TunnelStatus::Stopped | TunnelStatus::Failed
            )
        {
            // `starting` is stoppable; anything else terminal returns as-is.
        }
        if stop_target(refreshed.status).is_none() {
            if matches!(
                refreshed.status,
                TunnelStatus::Stopped | TunnelStatus::Failed
            ) {
                return Ok(refreshed);
            }
            return Err(TunnelError::new(
                TunnelProblemCode::Conflict,
                format!("tunnel {id} is {:?}; nothing to stop", refreshed.status),
            ));
        }
        if !self.sessions.contains_key(id)
            && !socket_owned
            && !live_exists
            && refreshed.origin == Some(TunnelOrigin::External)
        {
            return Err(TunnelError::new(
                TunnelProblemCode::Conflict,
                "this tunnel was started outside the host; stop it where it was started",
            ));
        }
        self.set_status(id, TunnelStatus::Stopping, |_| {})?;
        if self.sessions.contains_key(id) || socket_owned {
            if let Some(control) = &self.control {
                control.exit(&self.control_path_for(id), &ssh_destination(&profile));
            }
            self.sessions.remove(id);
        }
        if let Some(mut live) = self.live.remove(id) {
            if let Some(child) = live.child.child.as_mut() {
                let _ = child.kill();
            } else {
                self.process.kill_tree(live.child.pid);
            }
            std::thread::sleep(Duration::from_millis(1200));
            if self.process.is_alive(live.child.pid) {
                if let Some(child) = live.child.child.as_mut() {
                    let _ = child.kill();
                }
                std::thread::sleep(Duration::from_millis(400));
            }
        } else if !self.multiplexed
            && refreshed.origin == Some(TunnelOrigin::Managed)
            && refreshed.pid.is_some_and(|pid| self.process.is_alive(pid))
        {
            let pid = refreshed.pid.unwrap_or(0);
            self.process.kill_tree(pid);
            std::thread::sleep(Duration::from_millis(1200));
        }
        // A stop is the user declaring the red text handled: the next glance
        // at the row must not show yesterday's failure as today's state.
        self.set_status(id, TunnelStatus::Stopped, |s| {
            s.pid = None;
            s.started_at = None;
            s.last_error = None;
        })?;
        // The master is gone, so its socket file must go too — otherwise the
        // next start finds a stale file and runs without multiplexing.
        if self.multiplexed {
            let socket = self.control_path_for(id);
            if !self.socket_alive(id, &profile) {
                let _ = std::fs::remove_file(&socket);
            }
        }
        append_log_line(
            &self.log_dir,
            id,
            &format!("--- stopped by user {} ---", self.clock.now_iso()),
        );
        self.persist_runtime();
        self.states.get(id).cloned().ok_or_else(|| {
            TunnelError::new(TunnelProblemCode::InternalError, "state lost during stop")
        })
    }

    pub fn stop_all(&mut self) -> Vec<TunnelState> {
        let _ = self.ensure_loaded();
        let ids: Vec<String> = self.states.keys().cloned().collect();
        let mut results = Vec::new();
        for id in ids {
            // One stuck tunnel must not block the rest.
            if let Ok(state) = self.stop(&id) {
                results.push(state);
            }
        }
        results
    }

    pub fn probe_tunnel(&mut self, id: &str) -> Result<TunnelState> {
        self.ensure_loaded()?;
        let profile = self.get_profile(id)?;
        let refreshed = self.refresh_state(id)?;
        if !matches!(
            refreshed.status,
            TunnelStatus::Running | TunnelStatus::Degraded
        ) {
            return Ok(refreshed);
        }
        let mut results: HashMap<u16, (bool, Option<u64>, Option<String>)> = HashMap::new();
        for rule in &profile.forwards {
            let (bind_address, bind_port) = match rule {
                ForwardRule::Remote { .. } => continue,
                ForwardRule::Local {
                    bind_address,
                    bind_port,
                    ..
                }
                | ForwardRule::Dynamic {
                    bind_address,
                    bind_port,
                } => (bind_address, *bind_port),
            };
            let host = match bind_address.as_deref().unwrap_or("") {
                "" | "*" | "0.0.0.0" => "127.0.0.1",
                other => other,
            };
            let mut outcome = self.prober.dial_tcp(host, bind_port, 2500);
            for _ in 0..2 {
                if outcome.ok {
                    break;
                }
                std::thread::sleep(Duration::from_millis(800));
                outcome = self.prober.dial_tcp(host, bind_port, 2500);
            }
            results.insert(bind_port, (outcome.ok, outcome.latency_ms, outcome.detail));
        }
        let forwards = health_for_probe(&profile, &results);
        let any_local = forwards.iter().any(|f| f.kind != ForwardKind::Remote);
        let all_ok = forwards.iter().all(|f| f.ok);
        let next = if !any_local || all_ok {
            TunnelStatus::Running
        } else {
            TunnelStatus::Degraded
        };
        let mut updated = refreshed.clone();
        updated.forwards = forwards;
        if next != refreshed.status && can_transition(refreshed.status, next) {
            updated.status = next;
        }
        self.states.insert(id.into(), updated.clone());
        Ok(updated)
    }

    pub fn log_tail(&mut self, id: &str, tail: usize) -> Result<LogTail> {
        self.ensure_loaded()?;
        if !self.profiles.contains_key(id) {
            return Err(TunnelError::new(
                TunnelProblemCode::NotFound,
                format!("no profile {id}"),
            ));
        }
        let path = self.log_dir.join(format!("{id}.log"));
        let metadata = std::fs::metadata(&path);
        let Ok(metadata) = metadata else {
            return Ok(LogTail {
                profile_id: id.into(),
                path,
                lines: Vec::new(),
                truncated: false,
                bytes: 0,
            });
        };
        let size = metadata.len();
        if size == 0 {
            return Ok(LogTail {
                profile_id: id.into(),
                path,
                lines: Vec::new(),
                truncated: false,
                bytes: 0,
            });
        }
        let start = size.saturating_sub(MAX_LOG_WINDOW);
        let raw = std::fs::read(&path).unwrap_or_default();
        let window = &raw[start as usize..];
        let mut text = String::from_utf8_lossy(window).into_owned();
        // A windowed read almost always starts mid-line; drop that fragment.
        if start > 0 {
            match text.find('\n') {
                Some(index) => text = text[index + 1..].to_string(),
                None => text.clear(),
            }
        }
        let mut all: Vec<String> = text.split('\n').map(str::to_string).collect();
        if all.last().is_some_and(|last| last.is_empty()) {
            all.pop();
        }
        let skip = all.len().saturating_sub(tail);
        let lines = all[skip..].to_vec();
        Ok(LogTail {
            profile_id: id.into(),
            path,
            truncated: start > 0 || all.len() > lines.len(),
            lines,
            bytes: size,
        })
    }

    // -- terminal sessions -------------------------------------------------

    pub fn list_terminals(&self) -> Vec<crate::ports::TerminalInfo> {
        self.terminal.detect()
    }

    pub fn launch_in_terminal(
        &mut self,
        id: &str,
        terminal_id: &str,
        mode: LaunchMode,
    ) -> Result<ExternalSession> {
        self.ensure_loaded()?;
        let profile = self.get_profile(id)?;
        self.reclaim_socket(id, &profile)?;
        let control_path = self.handoff_control_path(id);
        let command = build_ssh_argv(
            &profile,
            ArgvOptions {
                control_path: control_path.as_deref(),
                interactive: mode == LaunchMode::Shell,
            },
        )
        .command;
        let pid = self
            .terminal
            .launch(terminal_id, &command)
            .map_err(|e| TunnelError::new(TunnelProblemCode::TerminalFailed, e.message.clone()))?;
        if pid == 0 {
            return Err(TunnelError::new(
                TunnelProblemCode::TerminalFailed,
                "the terminal launcher reported no process",
            ));
        }
        if let Some(previous) = self.sessions.get(id) {
            self.terminal.close(previous.pid);
        }
        let session = ExternalSession {
            profile_id: id.into(),
            terminal_id: terminal_id.into(),
            mode,
            command,
            pid,
            launched_at: self.clock.now_iso(),
        };
        self.sessions.insert(id.into(), session.clone());
        if let Some(state) = self.states.get_mut(id) {
            state.status = TunnelStatus::Starting;
            state.origin = Some(TunnelOrigin::Terminal);
            state.pid = None;
            state.started_at = Some(session.launched_at.clone());
            state.forwards = Vec::new();
            state.last_error = None;
        }
        self.persist_runtime();
        Ok(session)
    }

    pub fn list_sessions(&self) -> Vec<ExternalSession> {
        self.sessions.values().cloned().collect()
    }

    // -- urls ---------------------------------------------------------------

    pub fn open_url(&self, url: &str) -> Result<String> {
        self.open_url.open(url)?;
        Ok(self.clock.now_iso())
    }

    /// End a launched terminal session. Returns false when none was tracked.
    pub fn close_session(&mut self, id: &str) -> bool {
        let Some(session) = self.sessions.remove(id) else {
            return false;
        };
        self.terminal.close(session.pid);
        if let Ok(profile) = self.get_profile(id) {
            if let Some(control) = &self.control {
                control.exit(&self.control_path_for(id), &ssh_destination(&profile));
            }
        }
        self.persist_runtime();
        true
    }

    /// Local bind ports across all profiles (for conflict display).
    pub fn local_bind_ports(&self, id: &str) -> Vec<u16> {
        self.profiles
            .get(id)
            .map(local_bind_ports)
            .unwrap_or_default()
    }

    pub fn ssh_path(&self) -> &str {
        &self.ssh_path
    }

    pub fn log_dir(&self) -> &Path {
        &self.log_dir
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::contract::{ForwardRule, TunnelMode, TunnelProfile};
    use crate::ports::{
        ClockPort, ControlPort, LiveChild, OpenUrlPort, ProbeOutcome, ProbePort, ProcessPort,
        RuntimeRecord, RuntimeStorePort, StorePort, TerminalInfo, TerminalPort,
    };
    use std::path::Path;
    use std::sync::{Arc, Mutex};

    #[derive(Clone)]
    struct FakeClock;

    impl ClockPort for FakeClock {
        fn now_iso(&self) -> String {
            "2026-01-01T00:00:00.000Z".into()
        }
    }

    struct FakeProcess {
        next_pid: u32,
        alive: Vec<u32>,
        pub spawned: Vec<Vec<String>>,
        pub killed: Vec<u32>,
    }

    impl ProcessPort for FakeProcess {
        fn spawn(&mut self, argv: &[String], _log_file: &Path) -> Result<LiveChild> {
            let pid = self.next_pid;
            self.next_pid += 1;
            self.alive.push(pid);
            self.spawned.push(argv.to_vec());
            Ok(LiveChild { pid, child: None })
        }

        fn is_alive(&self, pid: u32) -> bool {
            self.alive.contains(&pid)
        }

        fn kill_tree(&mut self, pid: u32) {
            self.killed.push(pid);
            self.alive.retain(|p| *p != pid);
        }
    }

    /// Ports the fake claims are held — everything else refuses. Empty by
    /// default, so plain lifecycle tests never trip the pre-start port check.
    struct FakeProbe {
        open: Vec<u16>,
    }

    impl ProbePort for FakeProbe {
        fn dial_tcp(&self, _host: &str, _port: u16, _timeout_ms: u64) -> ProbeOutcome {
            ProbeOutcome {
                ok: true,
                latency_ms: Some(1),
                detail: None,
            }
        }

        fn port_held(&self, _host: &str, port: u16) -> bool {
            self.open.contains(&port)
        }
    }

    #[derive(Default)]
    struct FakeStore {
        saved: Vec<TunnelProfile>,
    }

    impl StorePort for FakeStore {
        fn read_profiles(&self) -> Result<Vec<TunnelProfile>> {
            Ok(self.saved.clone())
        }

        fn write_profiles(&self, profiles: &[TunnelProfile]) -> Result<()> {
            // Interior mutability is faked through the manager borrow; tests
            // write through a shared handle instead.
            let _ = profiles;
            Ok(())
        }
    }

    #[derive(Default)]
    struct FakeRuntime {
        records: Vec<RuntimeRecord>,
    }

    impl RuntimeStorePort for FakeRuntime {
        fn read(&self) -> Vec<RuntimeRecord> {
            self.records.clone()
        }

        fn write(&self, _records: &[RuntimeRecord]) {}
    }

    struct FakeTerminal;

    impl TerminalPort for FakeTerminal {
        fn detect(&self) -> Vec<TerminalInfo> {
            Vec::new()
        }

        fn launch(&mut self, _terminal_id: &str, _command_line: &str) -> Result<u32> {
            Err(TunnelError::new(
                TunnelProblemCode::UnsupportedOperation,
                "no terminal",
            ))
        }

        fn close(&mut self, _pid: u32) {}
    }

    struct FakeOpenUrl;

    impl OpenUrlPort for FakeOpenUrl {
        fn open(&self, _url: &str) -> Result<()> {
            Ok(())
        }
    }

    struct FakeControl {
        alive: bool,
    }

    impl ControlPort for FakeControl {
        fn check(&self, _socket_path: &Path, _destination: &str) -> bool {
            self.alive
        }

        fn exit(&self, _socket_path: &Path, _destination: &str) -> bool {
            true
        }
    }

    fn input() -> TunnelProfile {
        TunnelProfile {
            id: String::new(),
            name: "demo".into(),
            mode: TunnelMode::Local,
            ssh_target: "ufo".into(),
            ssh_port: None,
            ssh_user: None,
            identity_file: None,
            jump_hops: None,
            forwards: vec![ForwardRule::Local {
                bind_address: None,
                bind_port: 18099,
                target_host: "127.0.0.1".into(),
                target_port: 22,
            }],
            extra_options: None,
            server_alive_interval: None,
            exit_on_forward_failure: None,
            clear_all_forwardings: None,
            no_remote_command: None,
            remote_command: None,
            auto_reconnect: None,
            notes: None,
            created_at: String::new(),
            updated_at: String::new(),
        }
    }

    #[allow(clippy::too_many_arguments)]
    fn manager() -> Manager<
        FakeProcess,
        FakeProbe,
        FakeStore,
        FakeClock,
        FakeRuntime,
        FakeTerminal,
        FakeOpenUrl,
        FakeControl,
    > {
        manager_in(
            PathBuf::from("/tmp/ssh-tunnel-test-logs"),
            PathBuf::from("/tmp/ssh-tunnel-test-control"),
            false,
            Vec::new(),
        )
    }

    #[allow(clippy::too_many_arguments)]
    fn manager_in(
        log_dir: PathBuf,
        control_dir: PathBuf,
        socket_alive: bool,
        held_ports: Vec<u16>,
    ) -> Manager<
        FakeProcess,
        FakeProbe,
        FakeStore,
        FakeClock,
        FakeRuntime,
        FakeTerminal,
        FakeOpenUrl,
        FakeControl,
    > {
        Manager::new(
            FakeProcess {
                next_pid: 50000,
                alive: Vec::new(),
                spawned: Vec::new(),
                killed: Vec::new(),
            },
            FakeProbe { open: held_ports },
            FakeStore::default(),
            FakeClock,
            FakeRuntime::default(),
            FakeTerminal,
            FakeOpenUrl,
            Some(FakeControl {
                alive: socket_alive,
            }),
            log_dir,
            control_dir,
            "ssh".into(),
        )
    }

    #[test]
    fn create_start_probe_stop_lifecycle() {
        let _guard = Arc::new(Mutex::new(()));
        let mut manager = manager();
        let created = manager.create_profile(input()).unwrap();
        let preview = manager.preview(&created.id).unwrap();
        assert!(preview.argv.contains(&"-L".to_string()));
        // Grace sleeps are real (900ms); the lifecycle test pays for one start.
        let started = manager.start(&created.id).unwrap();
        assert_eq!(started.status, TunnelStatus::Running);
        let probed = manager.probe_tunnel(&created.id).unwrap();
        assert_eq!(probed.status, TunnelStatus::Running);
        let stopped = manager.stop(&created.id).unwrap();
        assert_eq!(stopped.status, TunnelStatus::Stopped);
    }

    #[test]
    fn double_start_conflicts() {
        let mut manager = manager();
        let created = manager.create_profile(input()).unwrap();
        manager.start(&created.id).unwrap();
        let again = manager.start(&created.id);
        assert!(matches!(again, Err(e) if e.code == TunnelProblemCode::Conflict));
    }

    #[test]
    fn blank_name_is_rejected() {
        let mut manager = manager();
        let mut bad = input();
        bad.name = "  ".into();
        let result = manager.create_profile(bad);
        assert!(matches!(result, Err(e) if e.code == TunnelProblemCode::InvalidRequest));
    }

    #[test]
    fn lan_unreachable_names_the_privacy_grant() {
        let dir = std::env::temp_dir().join(format!("ssh-tunnel-hint-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let log = dir.join("t-1.log");
        std::fs::write(
            &log,
            "ssh: connect to host 192.168.6.250 port 22: No route to host\n",
        )
        .unwrap();
        let hint = super::startup_failure_hint(&log).expect("a LAN failure must hint");
        assert!(hint.contains("Local Network"), "{hint}");
        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn stale_socket_is_reclaimed_before_handoff() {
        let base = std::env::temp_dir().join(format!("ssh-tunnel-reclaim-{}", std::process::id()));
        let control_dir = base.join("control");
        std::fs::create_dir_all(&control_dir).unwrap();
        let mut manager = manager_in(base.join("logs"), control_dir.clone(), false, Vec::new());
        let created = manager.create_profile(input()).unwrap();
        // A dead master left its file behind; the socket answers nothing.
        let socket = control_dir.join(format!("{}.sock", created.id));
        std::fs::write(&socket, "stale").unwrap();
        // reclaim_socket runs before the terminal launcher, so the handoff
        // must get past it (and then fail on the fake terminal, proving the
        // stale file was not the thing that stopped it).
        let result = manager.launch_in_terminal(
            &created.id,
            "terminal",
            crate::contract::LaunchMode::Tunnel,
        );
        assert!(!socket.exists(), "stale socket must be gone");
        assert!(matches!(result, Err(e) if e.code == TunnelProblemCode::TerminalFailed));
        std::fs::remove_dir_all(&base).unwrap();
    }

    #[test]
    fn live_socket_is_never_stolen() {
        let base = std::env::temp_dir().join(format!("ssh-tunnel-steal-{}", std::process::id()));
        let control_dir = base.join("control");
        std::fs::create_dir_all(&control_dir).unwrap();
        let mut manager = manager_in(base.join("logs"), control_dir.clone(), true, Vec::new());
        let created = manager.create_profile(input()).unwrap();
        let socket = control_dir.join(format!("{}.sock", created.id));
        std::fs::write(&socket, "live").unwrap();
        let result = manager.launch_in_terminal(
            &created.id,
            "terminal",
            crate::contract::LaunchMode::Tunnel,
        );
        assert!(matches!(result, Err(e) if e.code == TunnelProblemCode::Conflict));
        assert!(socket.exists(), "a live socket must survive");
        std::fs::remove_dir_all(&base).unwrap();
    }

    #[test]
    fn held_bind_port_fails_fast_with_a_human_message() {
        let base = std::env::temp_dir().join(format!("ssh-tunnel-held-{}", std::process::id()));
        // Port 18099 is the fixture profile's bind port; something already
        // holds it (an orphan ssh, a second instance), so ssh would die on
        // `Address already in use` — say so up front instead.
        let mut manager = manager_in(base.join("logs"), base.join("control"), false, vec![18099]);
        let created = manager.create_profile(input()).unwrap();
        // Like every other substantive start failure in this manager (spawn
        // refused, early exit), a held port answers Ok with a failed state —
        // only illegal transitions throw.
        let state = manager.start(&created.id).unwrap();
        assert_eq!(state.status, TunnelStatus::Failed);
        assert!(
            state
                .last_error
                .as_ref()
                .is_some_and(|message| message.contains("already in use")),
            "unexpected message: {:?}",
            state.last_error
        );
        std::fs::remove_dir_all(&base).unwrap();
    }

    #[test]
    fn stop_clears_a_previous_failure() {
        let mut manager = manager();
        let created = manager.create_profile(input()).unwrap();
        // Simulate yesterday's red text surviving on a live row: the failure
        // predates this run, and stop is the user declaring it handled.
        manager
            .set_status(&created.id, TunnelStatus::Starting, |_| {})
            .unwrap();
        manager
            .set_status(&created.id, TunnelStatus::Running, |s| {
                s.last_error = Some("old failure".into());
            })
            .unwrap();
        let stopped = manager.stop(&created.id).unwrap();
        assert_eq!(stopped.status, TunnelStatus::Stopped);
        assert_eq!(stopped.last_error, None);
    }

    #[test]
    fn non_lan_failures_keep_the_plain_message() {
        let dir = std::env::temp_dir().join(format!("ssh-tunnel-nohint-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let log = dir.join("t-1.log");
        std::fs::write(
            &log,
            "ssh: connect to host example.com port 22: Operation timed out\n",
        )
        .unwrap();
        assert!(super::startup_failure_hint(&log).is_none());
        std::fs::remove_dir_all(&dir).unwrap();
    }
}
