#![forbid(unsafe_code)]
//! Closed problem code table for the tunnel manager.
//!
//! Mirrors `TUNNEL_PROBLEM_CODES` in `packages/ssh-contract`. The set is
//! closed on purpose: clients branch on the variant, never on message text,
//! and one variant always maps to one HTTP status. There is no catch-all —
//! unknown wire codes are rejected at deserialization.

use serde::{Deserialize, Serialize};

/// Every failure a client can see. Serde keeps kebab-free PascalCase names so
/// the wire spelling matches the TypeScript table exactly.
#[derive(Clone, Copy, Debug, Deserialize, Eq, Hash, PartialEq, Serialize)]
#[non_exhaustive]
pub enum TunnelProblemCode {
    InvalidRequest,
    NotFound,
    Conflict,
    ResourceBusy,
    LimitExceeded,
    SshNotFound,
    SshFailed,
    PortInUse,
    ProbeFailed,
    StoreFailed,
    TerminalFailed,
    UnsupportedOperation,
    Timeout,
    InternalError,
}

impl TunnelProblemCode {
    /// The HTTP status this code always maps to. Exhaustive by construction.
    pub fn http_status(self) -> u16 {
        match self {
            Self::InvalidRequest => 400,
            Self::NotFound => 404,
            Self::Conflict => 409,
            Self::ResourceBusy => 429,
            Self::LimitExceeded => 429,
            Self::SshNotFound => 503,
            Self::SshFailed => 502,
            Self::PortInUse => 409,
            Self::ProbeFailed => 502,
            Self::StoreFailed => 500,
            Self::TerminalFailed => 502,
            Self::UnsupportedOperation => 400,
            Self::Timeout => 504,
            Self::InternalError => 500,
        }
    }
}

/// A code-carrying domain error. Messages are complete sentences; callers
/// branch on `code`, never on `message`.
#[derive(Clone, Debug, PartialEq, thiserror::Error)]
#[error("{code:?}: {message}")]
#[non_exhaustive]
pub struct TunnelError {
    pub code: TunnelProblemCode,
    pub message: String,
    pub retryable: bool,
}

impl TunnelError {
    pub fn new(code: TunnelProblemCode, message: impl Into<String>) -> Self {
        Self {
            code,
            message: message.into(),
            retryable: false,
        }
    }

    pub fn retryable(mut self) -> Self {
        self.retryable = true;
        self
    }
}

pub type Result<T> = std::result::Result<T, TunnelError>;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_code_maps_to_its_documented_status() {
        let cases = [
            (TunnelProblemCode::InvalidRequest, 400),
            (TunnelProblemCode::NotFound, 404),
            (TunnelProblemCode::Conflict, 409),
            (TunnelProblemCode::ResourceBusy, 429),
            (TunnelProblemCode::LimitExceeded, 429),
            (TunnelProblemCode::SshNotFound, 503),
            (TunnelProblemCode::SshFailed, 502),
            (TunnelProblemCode::PortInUse, 409),
            (TunnelProblemCode::ProbeFailed, 502),
            (TunnelProblemCode::StoreFailed, 500),
            (TunnelProblemCode::TerminalFailed, 502),
            (TunnelProblemCode::UnsupportedOperation, 400),
            (TunnelProblemCode::Timeout, 504),
            (TunnelProblemCode::InternalError, 500),
        ];
        for (code, status) in cases {
            assert_eq!(code.http_status(), status, "{code:?}");
        }
    }

    #[test]
    fn wire_spelling_matches_the_typescript_table() {
        // The TypeScript table is the source of truth; serde must emit it verbatim.
        let json = serde_json::to_string(&TunnelProblemCode::SshNotFound).unwrap();
        assert_eq!(json, "\"SshNotFound\"");
    }
}
