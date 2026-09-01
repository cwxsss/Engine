//! Read-only sponsor-side pairing diagnostics.

use uc_application::facade::{AppFacade, PairingDiagnosticsView};

use crate::{
    EngineError, OperationResult, PairingCandidateDiagnosticSummary, PairingDiagnosticsSummary,
    PairingInboundDiagnosticsSummary,
};

pub async fn execute_query_pairing_diagnostics(
    facade: &AppFacade,
) -> Result<OperationResult, EngineError> {
    let diagnostics = facade.pairing_diagnostics().await;
    Ok(OperationResult::PairingDiagnostics(summary(diagnostics)))
}

fn summary(diagnostics: PairingDiagnosticsView) -> PairingDiagnosticsSummary {
    PairingDiagnosticsSummary {
        candidates: diagnostics
            .candidates
            .into_iter()
            .map(|candidate| PairingCandidateDiagnosticSummary {
                kind: candidate.kind,
                address_hint: candidate.address_hint,
                port: candidate.port,
            })
            .collect(),
        inbound: PairingInboundDiagnosticsSummary {
            events_delivered: diagnostics.inbound.events_delivered,
            last_stage: diagnostics.inbound.last_stage,
            last_stage_elapsed_ms: diagnostics.inbound.last_stage_elapsed_ms,
            last_failure: diagnostics.inbound.last_failure,
        },
    }
}
