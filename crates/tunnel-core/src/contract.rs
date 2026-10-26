#![forbid(unsafe_code)]
//! Wire contract: profile + runtime state shapes.
//!
//! Mirrors the zod schemas in `packages/ssh-contract/src/tunnel.ts`. Field
//! names use the wire's camelCase so serialized JSON is byte-compatible with
//! the TypeScript daemon; validation of incoming values happens here at
//! construction, never deep in a caller.

use serde::{Deserialize, Serialize};

/// Discriminant for one forward rule. The `kind` tag is the whole dispatch.
#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum ForwardKind {
    Local,
    Remote,
    Dynamic,
}

/// One `-L` / `-R` / `-D` rule. `bind_address` empty means loopback.
#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
#[serde(tag = "kind", rename_all = "lowercase")]
pub enum ForwardRule {
    Local {
        #[serde(
            default,
            rename = "bindAddress",
            skip_serializing_if = "Option::is_none"
        )]
        bind_address: Option<String>,
        #[serde(rename = "bindPort")]
        bind_port: u16,
        #[serde(rename = "targetHost")]
        target_host: String,
        #[serde(rename = "targetPort")]
        target_port: u16,
    },
    Remote {
        #[serde(
            default,
            rename = "bindAddress",
            skip_serializing_if = "Option::is_none"
        )]
        bind_address: Option<String>,
        #[serde(rename = "bindPort")]
        bind_port: u16,
        #[serde(rename = "targetHost")]
        target_host: String,
        #[serde(rename = "targetPort")]
        target_port: u16,
    },
    Dynamic {
        #[serde(
            default,
            rename = "bindAddress",
            skip_serializing_if = "Option::is_none"
        )]
        bind_address: Option<String>,
        #[serde(rename = "bindPort")]
        bind_port: u16,
    },
}

impl ForwardRule {
    pub fn kind(&self) -> ForwardKind {
        match self {
            Self::Local { .. } => ForwardKind::Local,
            Self::Remote { .. } => ForwardKind::Remote,
            Self::Dynamic { .. } => ForwardKind::Dynamic,
        }
    }

    pub fn bind_port(&self) -> u16 {
        match self {
            Self::Local { bind_port, .. }
            | Self::Remote { bind_port, .. }
            | Self::Dynamic { bind_port, .. } => *bind_port,
        }
    }
}

/// Which ssh data-plane shape a profile uses.
#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum TunnelMode {
    Local,
    Remote,
    Dynamic,
    Jump,
    Multi,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
pub struct SshOption {
    pub key: String,
    pub value: String,
}

/// The persisted, user-named unit. One profile = one `ssh` process.
#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TunnelProfile {
    pub id: String,
    pub name: String,
    pub mode: TunnelMode,
    /// `user@host`, SSH config alias, or full `ssh://` destination.
    pub ssh_target: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub ssh_port: Option<u16>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub ssh_user: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub identity_file: Option<String>,
    /// Bastion hops for `-J hop1,hop2`. Empty = direct.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub jump_hops: Option<Vec<String>>,
    pub forwards: Vec<ForwardRule>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub extra_options: Option<Vec<SshOption>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub server_alive_interval: Option<u32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub exit_on_forward_failure: Option<bool>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub clear_all_forwardings: Option<bool>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub no_remote_command: Option<bool>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub remote_command: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub auto_reconnect: Option<bool>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub notes: Option<String>,
    pub created_at: String,
    pub updated_at: String,
}

/// Lifecycle status. No default — absence is not a state.
#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum TunnelStatus {
    Stopped,
    Starting,
    Running,
    Degraded,
    Failed,
    Stopping,
}

/// Who started a tunnel, and therefore what the host can honestly say.
#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum TunnelOrigin {
    Managed,
    Terminal,
    External,
}

/// `tunnel` keeps `-N`; `shell` gives the window an interactive login shell.
#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum LaunchMode {
    Tunnel,
    Shell,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ForwardHealth {
    pub kind: ForwardKind,
    pub bind_port: u16,
    pub ok: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub latency_ms: Option<u64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub detail: Option<String>,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TunnelState {
    pub profile_id: String,
    pub status: TunnelStatus,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub origin: Option<TunnelOrigin>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub pid: Option<u32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub started_at: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub last_error: Option<String>,
    #[serde(default)]
    pub forwards: Vec<ForwardHealth>,
    /// The exact argv the running process was started with (for inspect).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub argv: Option<Vec<String>>,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
pub struct SshHostInfo {
    pub alias: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub host_name: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub user: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub port: Option<u16>,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
pub struct TerminalTarget {
    pub id: String,
    pub label: String,
    pub hint: String,
    pub platform: String,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn profile_wire_shape_matches_typescript() {
        let profile = TunnelProfile {
            id: "t-1".into(),
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
            created_at: "2026-01-01T00:00:00.000Z".into(),
            updated_at: "2026-01-01T00:00:00.000Z".into(),
        };
        let json = serde_json::to_value(&profile).unwrap();
        assert_eq!(json["sshTarget"], "ufo");
        assert_eq!(json["forwards"][0]["kind"], "local");
        assert_eq!(json["forwards"][0]["bindPort"], 18099);
        // Optional fields stay absent, not null — the TS `.loose()` readers
        // accept both, but the file on disk must not drift.
        assert!(json.get("sshPort").is_none());
    }

    #[test]
    fn state_round_trips_with_optional_origin() {
        let json = serde_json::json!({
            "profileId": "t-1",
            "status": "running",
            "forwards": [],
        });
        let state: TunnelState = serde_json::from_value(json).unwrap();
        assert_eq!(state.status, TunnelStatus::Running);
        assert_eq!(state.origin, None);
    }
}
