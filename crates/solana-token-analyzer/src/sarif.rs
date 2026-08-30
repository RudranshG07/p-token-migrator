//! SARIF (Static Analysis Results Interchange Format) 2.1.0 output.
//!
//! GitHub's code-scanning surface ingests SARIF directly: upload it via
//! `github/codeql-action/upload-sarif@v3` in a workflow and findings show up
//! inline on PR diffs as annotations. That's how `sta` becomes a
//! continuous-use surface instead of a one-shot CLI.

use serde::Serialize;

use crate::manifest::{Finding, Manifest, Operation, RiskLevel};

const SARIF_SCHEMA: &str = "https://json.schemastore.org/sarif-2.1.0.json";
const SARIF_VERSION: &str = "2.1.0";
const TOOL_NAME: &str = "solana-token-analyzer";
const TOOL_VERSION: &str = env!("CARGO_PKG_VERSION");
const TOOL_INFO_URI: &str = "https://github.com/RudyG07/p-token-migrator";

#[derive(Debug, Serialize)]
pub struct Sarif {
    #[serde(rename = "$schema")]
    schema: &'static str,
    version: &'static str,
    runs: Vec<SarifRun>,
}

#[derive(Debug, Serialize)]
struct SarifRun {
    tool: SarifTool,
    results: Vec<SarifResult>,
}

#[derive(Debug, Serialize)]
struct SarifTool {
    driver: SarifDriver,
}

#[derive(Debug, Serialize)]
struct SarifDriver {
    name: &'static str,
    version: &'static str,
    #[serde(rename = "informationUri")]
    information_uri: &'static str,
    rules: Vec<SarifRule>,
}

#[derive(Debug, Serialize)]
struct SarifRule {
    id: String,
    name: String,
    #[serde(rename = "shortDescription")]
    short_description: SarifText,
    #[serde(rename = "fullDescription")]
    full_description: SarifText,
    #[serde(rename = "helpUri")]
    help_uri: &'static str,
    #[serde(rename = "defaultConfiguration")]
    default_configuration: SarifConfig,
}

#[derive(Debug, Serialize)]
struct SarifConfig {
    level: &'static str,
}

#[derive(Debug, Serialize)]
struct SarifText {
    text: String,
}

#[derive(Debug, Serialize)]
struct SarifResult {
    #[serde(rename = "ruleId")]
    rule_id: String,
    level: &'static str,
    message: SarifText,
    locations: Vec<SarifLocation>,
}

#[derive(Debug, Serialize)]
struct SarifLocation {
    #[serde(rename = "physicalLocation")]
    physical_location: SarifPhysical,
}

#[derive(Debug, Serialize)]
struct SarifPhysical {
    #[serde(rename = "artifactLocation")]
    artifact_location: SarifArtifact,
    region: SarifRegion,
}

#[derive(Debug, Serialize)]
struct SarifArtifact {
    uri: String,
}

#[derive(Debug, Serialize)]
struct SarifRegion {
    #[serde(rename = "startLine")]
    start_line: usize,
}

pub fn manifest_to_sarif(manifest: &Manifest) -> Sarif {
    let rules = Operation::ALL.iter().map(|op| make_rule(*op)).collect();
    let results = manifest.findings.iter().map(make_result).collect();
    Sarif {
        schema: SARIF_SCHEMA,
        version: SARIF_VERSION,
        runs: vec![SarifRun {
            tool: SarifTool {
                driver: SarifDriver {
                    name: TOOL_NAME,
                    version: TOOL_VERSION,
                    information_uri: TOOL_INFO_URI,
                    rules,
                },
            },
            results,
        }],
    }
}

fn make_rule(op: Operation) -> SarifRule {
    let id = format!("spl-token-{}", op.as_str());
    let name = format!("SplToken{}", op.pascal());
    let short = format!("SPL Token `{}` CPI call site", op.as_str());
    let full = format!(
        "Detects an SPL Token `{}` CPI call site that is a candidate for migration to p-token / verification under Token-2022. The analyzer emits the legacy and target compute-unit costs plus a generated replacement snippet alongside each finding.",
        op.as_str()
    );
    SarifRule {
        id,
        name,
        short_description: SarifText { text: short },
        full_description: SarifText { text: full },
        help_uri: TOOL_INFO_URI,
        default_configuration: SarifConfig { level: "warning" },
    }
}

fn make_result(finding: &Finding) -> SarifResult {
    let level = match finding.risk.level {
        RiskLevel::High => "error",
        RiskLevel::Medium => "warning",
        RiskLevel::Low => "note",
    };
    let message = format!(
        "{op}: {reason}. Legacy CU {legacy} → target {target} (saved {saved}, {pct}%).",
        op = finding.operation.as_str(),
        reason = finding.risk.reason,
        legacy = finding.compute.legacy_cu,
        target = finding.compute.p_token_cu,
        saved = finding.compute.saved_cu,
        pct = finding.compute.savings_percent,
    );
    SarifResult {
        rule_id: format!("spl-token-{}", finding.operation.as_str()),
        level,
        message: SarifText { text: message },
        locations: vec![SarifLocation {
            physical_location: SarifPhysical {
                artifact_location: SarifArtifact {
                    uri: finding.file.clone(),
                },
                region: SarifRegion {
                    start_line: finding.line,
                },
            },
        }],
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::manifest::{
        Compute, Confidence, Finding, Manifest, PatternKind, ProfileSummary, Risk, SimulationReport,
        SimulationStatus, TokenProgram, Totals,
    };

    fn fake_manifest() -> Manifest {
        Manifest {
            schema_version: "0.2.0".to_string(),
            generated_at: "2026-05-13T00:00:00Z".to_string(),
            protocol: "Test".to_string(),
            root: "test".to_string(),
            p_token_profile: ProfileSummary {
                name: "p".to_string(),
                status: "s".to_string(),
                note: "n".to_string(),
            },
            totals: Totals {
                legacy_cu: 5200,
                p_token_cu: 220,
                saved_cu: 4980,
                savings_percent: 95.8,
                call_sites: 1,
                files_scanned: 1,
            },
            idl_hints: vec![],
            findings: vec![Finding {
                id: "lib.rs:17:transfer".to_string(),
                file: "lib.rs".to_string(),
                line: 17,
                operation: Operation::Transfer,
                snippet: "token::transfer(ctx, amount)?;".to_string(),
                confidence: Confidence::High,
                compute: Compute {
                    legacy_cu: 5200,
                    p_token_cu: 220,
                    saved_cu: 4980,
                    savings_percent: 95.8,
                },
                risk: Risk {
                    level: RiskLevel::High,
                    reason: "signer seeds present".to_string(),
                },
                replacement_patch: String::new(),
                token_program: TokenProgram::SplToken,
                pattern_kind: PatternKind::AnchorCpi,
                via_wrapper: None,
            }],
            simulation: SimulationReport {
                status: SimulationStatus::Passed,
                mode: "deterministic".to_string(),
                legacy_runs: 1,
                p_token_runs: 1,
                divergences: vec![],
            },
            milestones: vec![],
        }
    }

    #[test]
    fn manifest_yields_one_rule_per_operation_and_one_result_per_finding() {
        let sarif = manifest_to_sarif(&fake_manifest());
        assert_eq!(sarif.runs.len(), 1);
        assert_eq!(sarif.runs[0].tool.driver.rules.len(), Operation::ALL.len());
        assert_eq!(sarif.runs[0].results.len(), 1);
        let result = &sarif.runs[0].results[0];
        assert_eq!(result.rule_id, "spl-token-transfer");
        assert_eq!(result.level, "error");
        assert_eq!(
            result.locations[0].physical_location.artifact_location.uri,
            "lib.rs"
        );
        assert_eq!(result.locations[0].physical_location.region.start_line, 17);
    }

    #[test]
    fn risk_levels_map_to_sarif_severity() {
        let mut manifest = fake_manifest();
        manifest.findings[0].risk.level = RiskLevel::Medium;
        let sarif = manifest_to_sarif(&manifest);
        assert_eq!(sarif.runs[0].results[0].level, "warning");

        manifest.findings[0].risk.level = RiskLevel::Low;
        let sarif = manifest_to_sarif(&manifest);
        assert_eq!(sarif.runs[0].results[0].level, "note");
    }

    #[test]
    fn schema_and_version_present() {
        let json = serde_json::to_value(manifest_to_sarif(&fake_manifest())).unwrap();
        assert_eq!(json["$schema"], SARIF_SCHEMA);
        assert_eq!(json["version"], SARIF_VERSION);
    }
}
