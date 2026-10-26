//! Golden fixtures adjudicating the TypeScript and Rust implementations.
//!
//! `fixtures/argv/*.json` pins the exact ssh argv both builders must produce;
//! `fixtures/state-machine.json` pins every allowed and rejected transition.
//! A change to either domain must update the fixture first, so the other
//! language fails loudly instead of drifting silently.

use serde::Deserialize;
use std::fs;
use tunnel_core::argv::{build_ssh_argv, ArgvOptions};
use tunnel_core::contract::{TunnelProfile, TunnelStatus};
use tunnel_core::state_machine::can_transition;

#[derive(Debug, Deserialize)]
struct ArgvFixture {
    name: String,
    profile: TunnelProfile,
    #[serde(default)]
    options: FixtureOptions,
    #[serde(default)]
    #[serde(rename = "mustContain")]
    must_contain: Vec<String>,
    #[serde(default)]
    #[serde(rename = "mustNotContain")]
    must_not_contain: Vec<String>,
}

#[derive(Debug, Default, Deserialize)]
struct FixtureOptions {
    #[serde(default, rename = "controlPath")]
    control_path: Option<String>,
    #[serde(default)]
    interactive: bool,
}

#[derive(Debug, Deserialize)]
struct StateMachineFixture {
    transitions: Vec<StateTransition>,
}

#[derive(Debug, Deserialize)]
struct StateTransition {
    from: TunnelStatus,
    to: TunnelStatus,
    allowed: bool,
}

fn fixture_dir() -> std::path::PathBuf {
    // Integration tests run with CWD = the crate dir.
    std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../fixtures")
}

#[test]
fn argv_fixtures_match_the_typescript_builder() {
    let dir = fixture_dir().join("argv");
    let mut files: Vec<_> = fs::read_dir(&dir)
        .expect("fixtures/argv must exist")
        .map(|entry| entry.unwrap().path())
        .filter(|path| path.extension().is_some_and(|ext| ext == "json"))
        .collect();
    files.sort();
    assert!(!files.is_empty(), "no argv fixtures found");
    for file in files {
        let raw = fs::read_to_string(&file).unwrap();
        let fixture: ArgvFixture = serde_json::from_str(&raw).unwrap();
        let options = ArgvOptions {
            control_path: fixture.options.control_path.as_deref(),
            interactive: fixture.options.interactive,
        };
        let built = build_ssh_argv(&fixture.profile, options);
        let joined = built.argv.join(" ");
        for token in &fixture.must_contain {
            assert!(joined.contains(token), "{}: expected {token}", fixture.name);
        }
        for token in &fixture.must_not_contain {
            assert!(
                !joined.contains(token),
                "{}: unexpected {token}",
                fixture.name
            );
        }
    }
}

#[test]
fn state_machine_fixtures_match_the_typescript_rules() {
    let raw = fs::read_to_string(fixture_dir().join("state-machine.json")).unwrap();
    let fixture: StateMachineFixture = serde_json::from_str(&raw).unwrap();
    assert!(!fixture.transitions.is_empty());
    for transition in &fixture.transitions {
        assert_eq!(
            can_transition(transition.from, transition.to),
            transition.allowed,
            "{:?} -> {:?}",
            transition.from,
            transition.to
        );
    }
}
