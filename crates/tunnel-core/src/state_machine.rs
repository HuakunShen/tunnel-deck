#![forbid(unsafe_code)]
//! Tunnel lifecycle state machine. Allowed transitions are data, not lore:
//! every layer consults `can_transition` instead of re-implementing the rules,
//! so an illegal transition fails the same way everywhere. Mirrors
//! `packages/ssh-core/src/state-machine.ts`; `fixtures/state-machine.json`
//! adjudicates both implementations.

use crate::contract::TunnelStatus;

fn successors(from: TunnelStatus) -> &'static [TunnelStatus] {
    use TunnelStatus as S;
    match from {
        S::Stopped => &[S::Starting],
        S::Starting => &[S::Running, S::Failed, S::Stopping],
        S::Running => &[S::Stopping, S::Degraded, S::Failed],
        S::Degraded => &[S::Running, S::Stopping, S::Failed],
        S::Failed => &[S::Starting, S::Stopped],
        S::Stopping => &[S::Stopped, S::Failed],
    }
}

pub fn can_transition(from: TunnelStatus, to: TunnelStatus) -> bool {
    successors(from).contains(&to)
}

/// The status one `start` request moves to from here, if any.
pub fn start_target(from: TunnelStatus) -> Option<TunnelStatus> {
    match from {
        TunnelStatus::Stopped | TunnelStatus::Failed => Some(TunnelStatus::Starting),
        _ => None,
    }
}

/// The status one `stop` request moves to from here, if any.
pub fn stop_target(from: TunnelStatus) -> Option<TunnelStatus> {
    match from {
        TunnelStatus::Starting | TunnelStatus::Running | TunnelStatus::Degraded => {
            Some(TunnelStatus::Stopping)
        }
        _ => None,
    }
}

pub fn is_startable(status: TunnelStatus) -> bool {
    start_target(status).is_some()
}

pub fn is_stoppable(status: TunnelStatus) -> bool {
    stop_target(status).is_some()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn stopped_only_starts() {
        assert_eq!(
            start_target(TunnelStatus::Stopped),
            Some(TunnelStatus::Starting)
        );
        assert_eq!(
            start_target(TunnelStatus::Failed),
            Some(TunnelStatus::Starting)
        );
        assert_eq!(start_target(TunnelStatus::Running), None);
        assert_eq!(start_target(TunnelStatus::Starting), None);
    }

    #[test]
    fn live_states_only_stop() {
        for status in [
            TunnelStatus::Starting,
            TunnelStatus::Running,
            TunnelStatus::Degraded,
        ] {
            assert_eq!(
                stop_target(status),
                Some(TunnelStatus::Stopping),
                "{status:?}"
            );
        }
        for status in [
            TunnelStatus::Stopped,
            TunnelStatus::Failed,
            TunnelStatus::Stopping,
        ] {
            assert_eq!(stop_target(status), None, "{status:?}");
        }
    }

    #[test]
    fn illegal_transitions_are_rejected() {
        assert!(!can_transition(
            TunnelStatus::Stopped,
            TunnelStatus::Running
        ));
        assert!(!can_transition(
            TunnelStatus::Running,
            TunnelStatus::Starting
        ));
        assert!(can_transition(
            TunnelStatus::Running,
            TunnelStatus::Degraded
        ));
    }
}
