#![forbid(unsafe_code)]
//! `ssh` argv builder — turns a validated profile into the exact argument
//! vector the host layer spawns. Pure function, no I/O. Mirrors
//! `packages/ssh-core/src/argv.ts` branch for branch; `fixtures/argv/*.json`
//! adjudicates both implementations so the documented command and the executed
//! command cannot drift apart.

use crate::contract::{ForwardRule, TunnelProfile};

/// Extras one specific run adds, never the advertised command.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct ArgvOptions<'a> {
    /// Absolute path of this tunnel's `ControlMaster` socket.
    pub control_path: Option<&'a str>,
    /// Give the terminal an interactive login shell instead of `-N`.
    pub interactive: bool,
}

#[derive(Clone, Debug, PartialEq)]
pub struct ArgvBuild {
    pub argv: Vec<String>,
    pub command: String,
}

/// Quote one argv element for display only (never for execution).
pub fn quote_for_display(argv: &[String]) -> String {
    let mut parts = vec!["ssh".to_string()];
    parts.extend(argv.iter().cloned());
    parts
        .iter()
        .map(|part| {
            if part.chars().all(|c| {
                c.is_ascii_alphanumeric()
                    || matches!(c, '_' | '@' | '%' | '+' | '=' | ':' | ',' | '.' | '/' | '-')
            }) && !part.is_empty()
            {
                part.clone()
            } else {
                format!("'{}'", part.replace('\'', "'\\''"))
            }
        })
        .collect::<Vec<_>>()
        .join(" ")
}

fn forward_spec(rule: &ForwardRule) -> String {
    let (bind_address, bind_port): (Option<&str>, u16) = match rule {
        ForwardRule::Local {
            bind_address,
            bind_port,
            ..
        }
        | ForwardRule::Remote {
            bind_address,
            bind_port,
            ..
        } => (bind_address.as_deref(), *bind_port),
        ForwardRule::Dynamic {
            bind_address,
            bind_port,
        } => (bind_address.as_deref(), *bind_port),
    };
    let bind = match bind_address {
        None | Some("") => String::new(),
        Some(addr) => format!("{addr}:"),
    };
    match rule {
        ForwardRule::Local {
            target_host,
            target_port,
            ..
        }
        | ForwardRule::Remote {
            target_host,
            target_port,
            ..
        } => {
            format!("{bind}{bind_port}:{target_host}:{target_port}")
        }
        ForwardRule::Dynamic { .. } => format!("{bind}{bind_port}"),
    }
}

fn flag_for(rule: &ForwardRule) -> &'static str {
    match rule {
        ForwardRule::Local { .. } => "-L",
        ForwardRule::Remote { .. } => "-R",
        ForwardRule::Dynamic { .. } => "-D",
    }
}

/// `user@host` as the profile asks for it; also what `-O` commands need.
pub fn ssh_destination(profile: &TunnelProfile) -> String {
    match &profile.ssh_user {
        Some(user) if !user.is_empty() => {
            let target = &profile.ssh_target;
            let host = target.split('@').next_back().unwrap_or(target);
            format!("{user}@{host}")
        }
        _ => profile.ssh_target.clone(),
    }
}

/// All local bind ports this profile will listen on (for conflict checks).
pub fn local_bind_ports(profile: &TunnelProfile) -> Vec<u16> {
    profile
        .forwards
        .iter()
        .filter(|rule| !matches!(rule, ForwardRule::Remote { .. }))
        .map(ForwardRule::bind_port)
        .collect()
}

/// Build the argv for `ssh` (without the executable name itself).
/// Callers must pass a validated profile; this function throws nothing.
pub fn build_ssh_argv(profile: &TunnelProfile, options: ArgvOptions<'_>) -> ArgvBuild {
    let mut argv: Vec<String> = Vec::new();
    let push = |argv: &mut Vec<String>, key: &str, value: &str| {
        argv.push(key.to_string());
        argv.push(value.to_string());
    };

    // Batch + non-interactive first so a missing key fails fast, not on a
    // prompt. An interactive shell is the exception: the user can answer a
    // password prompt there, and ssh keeps the *first* value it sees for a
    // repeated option.
    if !options.interactive {
        push(&mut argv, "-o", "BatchMode=yes");
    }
    push(&mut argv, "-o", "StrictHostKeyChecking=accept-new");

    if profile.exit_on_forward_failure != Some(false) {
        push(&mut argv, "-o", "ExitOnForwardFailure=yes");
    }
    if profile.clear_all_forwardings == Some(true) {
        push(&mut argv, "-o", "ClearAllForwardings=yes");
    }

    let keepalive = profile.server_alive_interval.unwrap_or(30);
    if keepalive > 0 {
        push(&mut argv, "-o", &format!("ServerAliveInterval={keepalive}"));
        push(&mut argv, "-o", "ServerAliveCountMax=3");
    }

    if let Some(port) = profile.ssh_port {
        argv.push("-p".to_string());
        argv.push(port.to_string());
    }
    if let Some(identity) = &profile.identity_file {
        if !identity.is_empty() {
            argv.push("-i".to_string());
            argv.push(identity.clone());
        }
    }
    if let Some(hops) = &profile.jump_hops {
        if !hops.is_empty() {
            argv.push("-J".to_string());
            argv.push(hops.join(","));
        }
    }

    for rule in &profile.forwards {
        argv.push(flag_for(rule).to_string());
        argv.push(forward_spec(rule));
    }

    if let Some(extra) = &profile.extra_options {
        for opt in extra {
            push(&mut argv, "-o", &format!("{}={}", opt.key, opt.value));
        }
    }

    if let Some(path) = options.control_path {
        if !path.is_empty() {
            push(&mut argv, "-o", "ControlMaster=yes");
            push(&mut argv, "-o", &format!("ControlPath={path}"));
            push(&mut argv, "-o", "ControlPersist=no");
        }
    }

    argv.push(ssh_destination(profile));

    if options.interactive {
        // No `-N`: ssh asks for the login shell, and the forwards live exactly
        // as long as that session does.
    } else if profile.no_remote_command == Some(false) {
        if let Some(command) = &profile.remote_command {
            argv.push(command.clone());
        } else {
            argv.push("-N".to_string());
        }
    } else {
        argv.push("-N".to_string());
    }

    let command = quote_for_display(&argv);
    ArgvBuild { argv, command }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::contract::{SshOption, TunnelMode};

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
    fn default_build_matches_documented_argv() {
        let built = build_ssh_argv(&profile(), ArgvOptions::default());
        assert!(built.argv.contains(&"-N".to_string()));
        assert!(built.argv.contains(&"-L".to_string()));
        assert!(built.command.starts_with("ssh "));
    }

    #[test]
    fn interactive_drops_batch_mode_and_n() {
        let built = build_ssh_argv(
            &profile(),
            ArgvOptions {
                control_path: None,
                interactive: true,
            },
        );
        assert!(!built.argv.iter().any(|a| a == "BatchMode=yes"));
        assert!(!built.argv.contains(&"-N".to_string()));
    }

    #[test]
    fn control_path_adds_exactly_the_multiplexing_trio() {
        let built = build_ssh_argv(
            &profile(),
            ArgvOptions {
                control_path: Some("/tmp/t-1.sock"),
                interactive: false,
            },
        );
        let joined = built.argv.join(" ");
        assert!(joined.contains("ControlMaster=yes"));
        assert!(joined.contains("ControlPath=/tmp/t-1.sock"));
        assert!(joined.contains("ControlPersist=no"));
    }

    #[test]
    fn user_prefixes_destination() {
        let mut p = profile();
        p.ssh_user = Some("hk".into());
        p.ssh_target = "mac-mini".into();
        assert_eq!(ssh_destination(&p), "hk@mac-mini");
    }

    #[test]
    fn extra_options_append_as_dash_o() {
        let mut p = profile();
        p.extra_options = Some(vec![SshOption {
            key: "Compression".into(),
            value: "yes".into(),
        }]);
        let built = build_ssh_argv(&p, ArgvOptions::default());
        assert!(built.argv.join(" ").contains("Compression=yes"));
    }
}
