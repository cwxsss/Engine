//! Non-persistent, privacy-preserving snapshots for sponsor-side pairing.

use std::net::IpAddr;

/// A locally visible, redacted invitation dial candidate.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PairingInvitationCandidateDiagnostic {
    pub kind: String,
    pub address_hint: String,
    pub port: u16,
}

/// Transient sponsor-side pairing event state.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PairingInboundDiagnosticsView {
    pub events_delivered: u32,
    pub last_stage: String,
    pub last_stage_elapsed_ms: u64,
    pub last_failure: Option<String>,
}

impl Default for PairingInboundDiagnosticsView {
    fn default() -> Self {
        Self {
            events_delivered: 0,
            last_stage: "idle".to_owned(),
            last_stage_elapsed_ms: 0,
            last_failure: None,
        }
    }
}

/// Local pairing diagnostics for support export and in-app troubleshooting.
///
/// The view is process-local only. It intentionally excludes invitation
/// codes, device identifiers, passphrases and full network addresses.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct PairingDiagnosticsView {
    pub candidates: Vec<PairingInvitationCandidateDiagnostic>,
    pub inbound: PairingInboundDiagnosticsView,
}

pub(crate) fn redact_invitation_candidate(
    ip: IpAddr,
    port: u16,
) -> PairingInvitationCandidateDiagnostic {
    let (kind, address_hint) = match ip {
        IpAddr::V4(address) => {
            let octets = address.octets();
            ("ipv4", format!("{}.{}.x.x", octets[0], octets[1]))
        }
        IpAddr::V6(address) => {
            let segments = address.segments();
            ("ipv6", format!("{:x}:{:x}:x:x::", segments[0], segments[1]))
        }
    };
    PairingInvitationCandidateDiagnostic {
        kind: kind.to_owned(),
        address_hint,
        port,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn candidate_redaction_never_exposes_a_full_ipv4_address() {
        let candidate = redact_invitation_candidate("192.168.12.34".parse().unwrap(), 4080);

        assert_eq!(candidate.kind, "ipv4");
        assert_eq!(candidate.address_hint, "192.168.x.x");
        assert_eq!(candidate.port, 4080);
    }
}
