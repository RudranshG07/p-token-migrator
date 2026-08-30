use serde::Serialize;

use crate::replay::{AccountDiff, BuildDiffOutcome, ChainOutcome, ReplayOutcome, ReplayResult};

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ReplayReport {
    pub program: String,
    pub rpc_url: String,
    pub generated_at: String,
    pub total: usize,
    pub matched: usize,
    pub diverged: usize,
    pub skipped: usize,
    pub entries: Vec<ReplayReportEntry>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ReplayReportEntry {
    pub signature: String,
    pub slot: u64,
    pub mainnet: SerializedMainnet,
    pub replay: SerializedReplay,
    pub divergence: Vec<String>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SerializedMainnet {
    pub success: bool,
    pub error: Option<String>,
    pub compute_units_consumed: Option<u64>,
    pub log_count: usize,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase", tag = "kind")]
pub enum SerializedReplay {
    Executed {
        success: bool,
        compute_units_consumed: u64,
        error: Option<String>,
        log_count: usize,
        writable_account_count: usize,
    },
    Skipped {
        reason: String,
    },
    Failed {
        reason: String,
    },
}

impl ReplayReport {
    pub fn from_outcomes(
        program: impl Into<String>,
        rpc_url: impl Into<String>,
        outcomes: Vec<ReplayOutcome>,
    ) -> Self {
        let mut matched = 0;
        let mut diverged = 0;
        let mut skipped = 0;
        let mut entries = Vec::with_capacity(outcomes.len());
        for outcome in outcomes {
            let entry = ReplayReportEntry {
                signature: outcome.signature,
                slot: outcome.slot,
                mainnet: SerializedMainnet::from(outcome.mainnet),
                replay: SerializedReplay::from(outcome.replay),
                divergence: outcome.divergence,
            };
            match &entry.replay {
                SerializedReplay::Skipped { .. } => skipped += 1,
                _ if entry.divergence.is_empty() => matched += 1,
                _ => diverged += 1,
            }
            entries.push(entry);
        }
        let total = entries.len();
        ReplayReport {
            program: program.into(),
            rpc_url: rpc_url.into(),
            generated_at: current_timestamp(),
            total,
            matched,
            diverged,
            skipped,
            entries,
        }
    }
}

impl From<ChainOutcome> for SerializedMainnet {
    fn from(o: ChainOutcome) -> Self {
        SerializedMainnet {
            success: o.success,
            error: o.error,
            compute_units_consumed: o.compute_units_consumed,
            log_count: o.log_count,
        }
    }
}

// ---------------------------------------------------------------------------
// Two-build diff report
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct BuildDiffReport {
    pub target_program: String,
    pub legacy_so: String,
    pub new_so: String,
    pub rpc_url: String,
    pub generated_at: String,
    pub total: usize,
    pub matched: usize,
    pub diverged: usize,
    pub skipped: usize,
    pub entries: Vec<BuildDiffEntry>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct BuildDiffEntry {
    pub signature: String,
    pub slot: u64,
    pub mainnet: SerializedMainnet,
    pub legacy: SerializedReplay,
    pub new: SerializedReplay,
    /// Diff of execution metadata (success, CU, error, log count).
    pub legacy_vs_new: Vec<String>,
    /// Per-account post-state divergences — the safety-critical signal.
    /// Empty when the two builds wrote bytewise-identical state to every
    /// writable account they touched.
    pub account_diffs: Vec<SerializedAccountDiff>,
    pub legacy_apply_warnings: Vec<String>,
    pub new_apply_warnings: Vec<String>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SerializedAccountDiff {
    pub pubkey: String,
    pub kind: String,
    pub lamports_legacy: Option<u64>,
    pub lamports_new: Option<u64>,
    pub data_len_legacy: Option<usize>,
    pub data_len_new: Option<usize>,
    pub owner_legacy: Option<String>,
    pub owner_new: Option<String>,
    pub first_diff_byte: Option<usize>,
}

impl From<AccountDiff> for SerializedAccountDiff {
    fn from(d: AccountDiff) -> Self {
        SerializedAccountDiff {
            pubkey: d.pubkey.to_string(),
            kind: d.kind,
            lamports_legacy: d.lamports_legacy,
            lamports_new: d.lamports_new,
            data_len_legacy: d.data_len_legacy,
            data_len_new: d.data_len_new,
            owner_legacy: d.owner_legacy.map(|p| p.to_string()),
            owner_new: d.owner_new.map(|p| p.to_string()),
            first_diff_byte: d.first_diff_byte,
        }
    }
}

impl BuildDiffReport {
    pub fn from_outcomes(
        target_program: impl Into<String>,
        legacy_so: impl Into<String>,
        new_so: impl Into<String>,
        rpc_url: impl Into<String>,
        outcomes: Vec<BuildDiffOutcome>,
    ) -> Self {
        let mut matched = 0;
        let mut diverged = 0;
        let mut skipped = 0;
        let mut entries = Vec::with_capacity(outcomes.len());
        for outcome in outcomes {
            let entry = BuildDiffEntry {
                signature: outcome.signature,
                slot: outcome.slot,
                mainnet: SerializedMainnet::from(outcome.mainnet),
                legacy: SerializedReplay::from(outcome.legacy),
                new: SerializedReplay::from(outcome.new),
                legacy_vs_new: outcome.legacy_vs_new,
                account_diffs: outcome
                    .account_diffs
                    .into_iter()
                    .map(SerializedAccountDiff::from)
                    .collect(),
                legacy_apply_warnings: outcome.legacy_apply_warnings,
                new_apply_warnings: outcome.new_apply_warnings,
            };
            let both_skipped = matches!(entry.legacy, SerializedReplay::Skipped { .. })
                && matches!(entry.new, SerializedReplay::Skipped { .. });
            // A tx counts as "matched" only when BOTH metadata and account
            // state are identical between the two builds. Account state is
            // the safety-critical leg of that check.
            if both_skipped {
                skipped += 1;
            } else if entry.legacy_vs_new.is_empty() && entry.account_diffs.is_empty() {
                matched += 1;
            } else {
                diverged += 1;
            }
            entries.push(entry);
        }
        let total = entries.len();
        BuildDiffReport {
            target_program: target_program.into(),
            legacy_so: legacy_so.into(),
            new_so: new_so.into(),
            rpc_url: rpc_url.into(),
            generated_at: current_timestamp(),
            total,
            matched,
            diverged,
            skipped,
            entries,
        }
    }
}

impl From<ReplayResult> for SerializedReplay {
    fn from(r: ReplayResult) -> Self {
        match r {
            ReplayResult::Executed {
                success,
                compute_units_consumed,
                logs,
                error,
                post_state,
            } => SerializedReplay::Executed {
                success,
                compute_units_consumed,
                error,
                log_count: logs.len(),
                writable_account_count: post_state.len(),
            },
            ReplayResult::Skipped { reason } => SerializedReplay::Skipped { reason },
            ReplayResult::Failed { reason } => SerializedReplay::Failed { reason },
        }
    }
}

fn current_timestamp() -> String {
    use std::time::{SystemTime, UNIX_EPOCH};
    let secs = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0);
    // Re-uses the same naive ISO formatter as solana-token-analyzer to avoid a
    // chrono dependency here.
    const SECONDS_PER_DAY: u64 = 86_400;
    let days = (secs / SECONDS_PER_DAY) as i64;
    let tod = secs % SECONDS_PER_DAY;
    let (y, m, d) = civil_from_days(days);
    format!(
        "{:04}-{:02}-{:02}T{:02}:{:02}:{:02}Z",
        y, m, d,
        tod / 3600,
        (tod % 3600) / 60,
        tod % 60
    )
}

fn civil_from_days(z: i64) -> (i32, u32, u32) {
    let z = z + 719_468;
    let era = if z >= 0 { z / 146_097 } else { (z - 146_096) / 146_097 };
    let doe = (z - era * 146_097) as u64;
    let yoe = (doe - doe / 1_460 + doe / 36_524 - doe / 146_096) / 365;
    let mut y = yoe as i64 + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let m = if mp < 10 { mp + 3 } else { mp - 9 };
    if m <= 2 { y += 1; }
    (y as i32, m as u32, d as u32)
}
