#![forbid(unsafe_code)]
//! Host ports — the seam where portable core meets the OS. Core plans what to
//! do; only the trusted host implements these. Every trait is small and comes
//! with an in-memory fake in tests. Mirrors
//! `packages/ssh-host-node/src/ports.ts`.

use crate::contract::{LaunchMode, TunnelProfile};
use crate::error::{Result, TunnelError, TunnelProblemCode};
use std::path::Path;
use std::process::Child;

/// A spawned `ssh`, still owned by this host.
pub struct LiveChild {
    pub pid: u32,
    pub child: Option<Child>,
}

/// Spawn `ssh` detached; resolve once the child exists (not when ready).
pub trait ProcessPort: Send {
    fn spawn(&mut self, argv: &[String], log_file: &Path) -> Result<LiveChild>;
    /// True while pid is alive.
    fn is_alive(&self, pid: u32) -> bool;
    /// Best-effort kill of one pid.
    fn kill_tree(&mut self, pid: u32);
}

#[derive(Clone, Debug)]
pub struct ProbeOutcome {
    pub ok: bool,
    pub latency_ms: Option<u64>,
    pub detail: Option<String>,
}

/// TCP dial with timeout; resolves ok/latency, never throws for refuse.
pub trait ProbePort: Send {
    fn dial_tcp(&self, host: &str, port: u16, timeout_ms: u64) -> ProbeOutcome;

    /// Whether something already listens on this bind port. A dial that
    /// answers is the only honest signal, so the default implementation is
    /// just that — but it is its own question, because health probes and the
    /// pre-start bind check read the same answer to opposite conclusions.
    fn port_held(&self, host: &str, port: u16) -> bool {
        self.dial_tcp(host, port, 400).ok
    }
}

pub trait StorePort: Send {
    fn read_profiles(&self) -> Result<Vec<TunnelProfile>>;
    fn write_profiles(&self, profiles: &[TunnelProfile]) -> Result<()>;
}

pub trait ClockPort: Send {
    fn now_iso(&self) -> String;
}

#[derive(Clone, Debug)]
pub struct SystemClock;

impl ClockPort for SystemClock {
    fn now_iso(&self) -> String {
        chrono::Utc::now().to_rfc3339_opts(chrono::SecondsFormat::Millis, true)
    }
}

/// One tunnel that was running when the host last wrote its runtime file.
#[derive(Clone, Debug, serde::Deserialize, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RuntimeRecord {
    pub profile_id: String,
    pub origin: RuntimeOrigin,
    pub started_at: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub pid: Option<u32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub mode: Option<LaunchMode>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub terminal_id: Option<String>,
}

#[derive(Clone, Copy, Debug, serde::Deserialize, serde::Serialize, Eq, PartialEq)]
#[serde(rename_all = "lowercase")]
pub enum RuntimeOrigin {
    Managed,
    Terminal,
}

/// Runtime records: best-effort persistence, never a reason to fail a boot.
pub trait RuntimeStorePort: Send {
    fn read(&self) -> Vec<RuntimeRecord>;
    fn write(&self, records: &[RuntimeRecord]);
}

/// OpenSSH control-socket operations. Both answer `false` rather than throwing,
/// because "the socket is gone" is an ordinary state.
pub trait ControlPort: Send {
    /// True while the master behind `socket_path` is running.
    fn check(&self, socket_path: &Path, destination: &str) -> bool;
    /// Ask the master to exit; true when OpenSSH accepted the request.
    fn exit(&self, socket_path: &Path, destination: &str) -> bool;
}

#[derive(Clone, Debug, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TerminalInfo {
    pub id: String,
    pub label: String,
    pub hint: String,
    pub platform: String,
}

/// A terminal session this host launched, so it can be ended on demand.
#[derive(Clone, Debug, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ExternalSession {
    pub profile_id: String,
    pub terminal_id: String,
    pub mode: LaunchMode,
    pub command: String,
    pub pid: u32,
    pub launched_at: String,
}

pub trait TerminalPort: Send {
    fn detect(&self) -> Vec<TerminalInfo>;
    /// Launch `command_line` inside `terminal_id`; resolves to the spawned pid.
    /// Errs when the launcher itself failed. Waits a short settle window,
    /// because a launcher that dies instantly is the only observable failure.
    fn launch(&mut self, terminal_id: &str, command_line: &str) -> Result<u32>;
    /// End a previously launched terminal session.
    fn close(&mut self, pid: u32);
}

pub trait OpenUrlPort: Send {
    fn open(&self, url: &str) -> Result<()>;
}

pub fn invalid_request(message: impl Into<String>) -> TunnelError {
    TunnelError::new(TunnelProblemCode::InvalidRequest, message)
}

pub fn not_found(message: impl Into<String>) -> TunnelError {
    TunnelError::new(TunnelProblemCode::NotFound, message)
}

pub fn conflict(message: impl Into<String>) -> TunnelError {
    TunnelError::new(TunnelProblemCode::Conflict, message)
}
