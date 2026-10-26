#![forbid(unsafe_code)]
//! Semantic validation for tunnel profiles — beyond what shapes can say.
//! Returns findings with field paths so forms can highlight them. Pure
//! function, host-free. Mirrors `packages/ssh-core/src/validate.ts`.

use crate::contract::{ForwardRule, TunnelProfile};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Severity {
    Error,
    Warning,
}

#[derive(Clone, Debug, PartialEq)]
pub struct ProfileFinding {
    pub path: String,
    pub message: String,
    pub severity: Severity,
}

fn error(path: impl Into<String>, message: impl Into<String>) -> ProfileFinding {
    ProfileFinding {
        path: path.into(),
        message: message.into(),
        severity: Severity::Error,
    }
}

fn warning(path: impl Into<String>, message: impl Into<String>) -> ProfileFinding {
    ProfileFinding {
        path: path.into(),
        message: message.into(),
        severity: Severity::Warning,
    }
}

pub const MAX_FORWARDS: usize = 16;
pub const MAX_JUMP_HOPS: usize = 8;
pub const MAX_EXTRA_OPTIONS: usize = 32;
pub const MAX_NOTES_LENGTH: usize = 2000;

/// Validate a profile; empty output = valid. Only `Error` findings block start.
pub fn validate_profile(profile: &TunnelProfile) -> Vec<ProfileFinding> {
    let mut findings = Vec::new();

    if profile.name.trim().is_empty() {
        findings.push(error("name", "name must not be blank"));
    }
    if profile.ssh_target.trim().is_empty() {
        findings.push(error(
            "sshTarget",
            "ssh target must not be blank (alias, user@host, or hostname)",
        ));
    }
    if profile.forwards.is_empty() {
        findings.push(error("forwards", "at least one forward rule is required"));
    }
    if profile.forwards.len() > MAX_FORWARDS {
        findings.push(error(
            "forwards",
            format!("at most {MAX_FORWARDS} forwards per tunnel"),
        ));
    }

    // Duplicate local bind ports inside one profile would make ssh fail.
    {
        use std::collections::HashMap;
        let mut seen: HashMap<u16, usize> = HashMap::new();
        for (index, rule) in profile.forwards.iter().enumerate() {
            if matches!(rule, ForwardRule::Remote { .. }) {
                continue;
            }
            let port = rule.bind_port();
            if let Some(first) = seen.get(&port) {
                findings.push(error(
                    format!("forwards.{index}.bindPort"),
                    format!("port {port} already used by forwards.{first} in this profile"),
                ));
            } else {
                seen.insert(port, index);
            }
        }
    }

    for (index, rule) in profile.forwards.iter().enumerate() {
        let ForwardRule::Remote { bind_address, .. } = rule else {
            continue;
        };
        let bind = bind_address.as_deref().unwrap_or("");
        if matches!(bind, "" | "localhost" | "127.0.0.1" | "::1") {
            findings.push(warning(
                format!("forwards.{index}.bindAddress"),
                "remote forward binds loopback on the server: only the server itself can connect. Use 0.0.0.0 (needs GatewayPorts on the server) to expose it.",
            ));
        }
    }

    for (index, rule) in profile.forwards.iter().enumerate() {
        if matches!(rule, ForwardRule::Remote { .. }) {
            continue;
        }
        if rule.bind_port() < 1024 {
            findings.push(warning(
                format!("forwards.{index}.bindPort"),
                format!(
                    "port {} is privileged (<1024) and needs root/admin to bind",
                    rule.bind_port()
                ),
            ));
        }
    }

    if profile.no_remote_command == Some(false) && profile.remote_command.is_none() {
        findings.push(warning(
            "remoteCommand",
            "noRemoteCommand is false but no remoteCommand is set; ssh would open a shell",
        ));
    }
    if profile
        .jump_hops
        .as_ref()
        .is_some_and(|hops| hops.len() > MAX_JUMP_HOPS)
    {
        findings.push(error(
            "jumpHops",
            format!("at most {MAX_JUMP_HOPS} jump hops"),
        ));
    }
    if profile
        .extra_options
        .as_ref()
        .is_some_and(|opts| opts.len() > MAX_EXTRA_OPTIONS)
    {
        findings.push(error(
            "extraOptions",
            format!("at most {MAX_EXTRA_OPTIONS} extra -o options"),
        ));
    }
    if profile
        .notes
        .as_ref()
        .is_some_and(|notes| notes.len() > MAX_NOTES_LENGTH)
    {
        findings.push(error("notes", "notes too long"));
    }

    findings
}

/// Cross-profile check: two profiles wanting the same local port.
pub fn find_port_conflicts(profiles: &[TunnelProfile]) -> Vec<(u16, Vec<String>)> {
    use std::collections::HashMap;
    let mut by_port: HashMap<u16, Vec<String>> = HashMap::new();
    for profile in profiles {
        for rule in &profile.forwards {
            if matches!(rule, ForwardRule::Remote { .. }) {
                continue;
            }
            by_port
                .entry(rule.bind_port())
                .or_default()
                .push(profile.id.clone());
        }
    }
    by_port
        .into_iter()
        .filter(|(_, ids)| ids.len() > 1)
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::contract::{ForwardRule, TunnelMode};

    fn profile() -> TunnelProfile {
        TunnelProfile {
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
        }
    }

    #[test]
    fn valid_profile_has_no_findings() {
        assert!(validate_profile(&profile()).is_empty());
    }

    #[test]
    fn blank_name_and_target_block() {
        let mut p = profile();
        p.name = "  ".into();
        p.ssh_target = String::new();
        let findings = validate_profile(&p);
        assert!(findings
            .iter()
            .any(|f| f.path == "name" && f.severity == Severity::Error));
        assert!(findings
            .iter()
            .any(|f| f.path == "sshTarget" && f.severity == Severity::Error));
    }

    #[test]
    fn duplicate_bind_ports_block() {
        let mut p = profile();
        p.forwards.push(ForwardRule::Dynamic {
            bind_address: None,
            bind_port: 18099,
        });
        let findings = validate_profile(&p);
        assert!(findings.iter().any(|f| f.severity == Severity::Error));
    }
}
