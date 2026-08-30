use serde::{Deserialize, Serialize};

pub const SCHEMA_VERSION: &str = "0.2.0";

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Operation {
    Transfer,
    MintTo,
    Burn,
    Approve,
    CloseAccount,
    InitializeAccount,
}

impl Operation {
    pub const ALL: &'static [Operation] = &[
        Operation::Transfer,
        Operation::MintTo,
        Operation::Burn,
        Operation::Approve,
        Operation::CloseAccount,
        Operation::InitializeAccount,
    ];

    pub fn as_str(self) -> &'static str {
        match self {
            Operation::Transfer => "transfer",
            Operation::MintTo => "mint_to",
            Operation::Burn => "burn",
            Operation::Approve => "approve",
            Operation::CloseAccount => "close_account",
            Operation::InitializeAccount => "initialize_account",
        }
    }

    pub fn pascal(self) -> &'static str {
        match self {
            Operation::Transfer => "Transfer",
            Operation::MintTo => "MintTo",
            Operation::Burn => "Burn",
            Operation::Approve => "Approve",
            Operation::CloseAccount => "CloseAccount",
            Operation::InitializeAccount => "InitializeAccount",
        }
    }

    pub fn from_name(name: &str) -> Option<Self> {
        match name {
            "transfer"
            | "transfer_checked"
            | "invoke_transfer"
            | "invoke_transfer_checked" => Some(Operation::Transfer),
            "mint_to" | "mint_to_checked" => Some(Operation::MintTo),
            "burn" | "burn_checked" => Some(Operation::Burn),
            "approve" | "approve_checked" => Some(Operation::Approve),
            "close_account" => Some(Operation::CloseAccount),
            "initialize_account"
            | "initialize_account2"
            | "initialize_account3" => Some(Operation::InitializeAccount),
            _ => None,
        }
    }
}

/// Which token-program family a call site targets. This is additive over the
/// legacy manifest shape and is the key axis for the broader token-program
/// analysis platform (token-2022 / interface-aware code paths look identical
/// to legacy SPL Token in shape but have very different runtime behavior).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TokenProgram {
    SplToken,
    SplToken2022,
    /// `anchor_spl::token_interface` — interface-aware code that already
    /// dispatches to either SPL Token or Token-2022 at runtime.
    TokenInterface,
    Unknown,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Confidence {
    High,
    Medium,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum RiskLevel {
    Low,
    Medium,
    High,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PatternKind {
    AnchorCpi,
    SplInstruction,
    TokenInterface,
    /// A call to a function the analyzer determined (via cross-file fixpoint
    /// analysis) transitively performs a token CPI. The wrapper's name and
    /// the inherited operation are surfaced in the finding.
    WrappedCpi,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Compute {
    pub legacy_cu: u64,
    pub p_token_cu: u64,
    pub saved_cu: u64,
    pub savings_percent: f64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Risk {
    pub level: RiskLevel,
    pub reason: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Finding {
    pub id: String,
    pub file: String,
    pub line: usize,
    pub operation: Operation,
    pub snippet: String,
    pub confidence: Confidence,
    pub compute: Compute,
    pub risk: Risk,
    pub replacement_patch: String,
    /// Which token-program family this CPI targets. Additive field beyond the
    /// 0.1.0 manifest shape — the TS scanner ignores it.
    pub token_program: TokenProgram,
    /// Whether the call was matched as an Anchor CPI helper, a raw
    /// `spl_token::instruction`, a `token_interface` dispatch site, or a
    /// transitive call to a wrapper function.
    pub pattern_kind: PatternKind,
    /// When `pattern_kind` is `WrappedCpi`, the simple name of the helper
    /// function that transitively performs the CPI. None for direct hits.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub via_wrapper: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct IdlHint {
    pub file: String,
    #[serde(rename = "type")]
    pub kind: String,
    pub message: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Totals {
    pub legacy_cu: u64,
    pub p_token_cu: u64,
    pub saved_cu: u64,
    pub savings_percent: f64,
    pub call_sites: usize,
    pub files_scanned: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ProfileSummary {
    pub name: String,
    pub status: String,
    pub note: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SimulationStatus {
    Passed,
    ReviewRequired,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Divergence {
    pub finding_id: String,
    pub severity: String,
    pub reason: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SimulationReport {
    pub status: SimulationStatus,
    pub mode: String,
    pub legacy_runs: usize,
    pub p_token_runs: usize,
    pub divergences: Vec<Divergence>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum MilestoneStatusLevel {
    Complete,
    Mvp,
    Blocked,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MilestoneStatus {
    pub name: String,
    pub status: MilestoneStatusLevel,
    pub evidence: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Manifest {
    pub schema_version: String,
    pub generated_at: String,
    pub protocol: String,
    pub root: String,
    pub p_token_profile: ProfileSummary,
    pub totals: Totals,
    pub idl_hints: Vec<IdlHint>,
    pub findings: Vec<Finding>,
    pub simulation: SimulationReport,
    pub milestones: Vec<MilestoneStatus>,
}
