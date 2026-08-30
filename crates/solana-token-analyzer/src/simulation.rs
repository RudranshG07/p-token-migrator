use crate::manifest::{
    Divergence, Finding, IdlHint, RiskLevel, SimulationReport, SimulationStatus,
};

/// Deterministic placeholder simulator. This will be removed once the
/// forked-mainnet replay (roadmap step 4) is in place. Until then it surfaces
/// the risk-classifier output through the same JSON shape so the dashboard
/// keeps working, but the field names are explicit about being heuristic
/// rather than executed.
pub fn run_dry_run(findings: &[Finding], idl_hints: &[IdlHint]) -> SimulationReport {
    let mut divergences = Vec::new();

    for finding in findings {
        if matches!(finding.risk.level, RiskLevel::High) {
            divergences.push(Divergence {
                finding_id: finding.id.clone(),
                severity: "review".to_string(),
                reason: finding.risk.reason.clone(),
            });
        }
    }

    if !idl_hints.is_empty() && findings.is_empty() {
        divergences.push(Divergence {
            finding_id: "idl-only".to_string(),
            severity: "review".to_string(),
            reason: "IDL references the legacy token program but no Rust CPI call site was found."
                .to_string(),
        });
    }

    let status = if divergences.is_empty() {
        SimulationStatus::Passed
    } else {
        SimulationStatus::ReviewRequired
    };

    SimulationReport {
        status,
        mode: "deterministic".to_string(),
        legacy_runs: findings.len(),
        p_token_runs: findings.len(),
        divergences,
    }
}
