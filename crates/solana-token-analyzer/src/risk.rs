use crate::manifest::{Operation, Risk, RiskLevel, TokenProgram};

/// Classify the risk of swapping a token CPI call site. Inputs:
///   - `file_source`: the full file body containing the call (used as a cheap
///     proxy for "does this function take signer seeds / a token_program
///     account?"). A future revision will replace this with proper enclosing-
///     function analysis once we have the call-graph walker.
///   - `snippet`: the single source line of the call site.
///   - `op`: the operation kind.
///   - `program`: which token-program family the call targets.
pub fn classify(
    file_source: &str,
    snippet: &str,
    op: Operation,
    program: TokenProgram,
) -> Risk {
    let snippet_lower = snippet.to_lowercase();
    let needs_signer_seeds = file_source.contains("with_signer")
        || file_source.contains("invoke_signed")
        || file_source.contains("signer_seeds")
        || file_source.contains("seeds = ")
        || file_source.contains("seeds=");
    let uses_token_program_account = file_source.contains("token_program");
    let is_checked = matches!(op, Operation::Transfer | Operation::MintTo | Operation::Burn | Operation::Approve)
        && snippet_lower.contains("checked");

    // Token-2022 / interface-aware code has the biggest behavioral surface
    // (transfer hooks, confidential transfers, memo requirement, etc.).
    if matches!(program, TokenProgram::SplToken2022 | TokenProgram::TokenInterface) {
        return Risk {
            level: RiskLevel::High,
            reason: "Token-2022 / interface-aware code path — verify transfer-hook, memo, and confidential-transfer extension behavior end-to-end.".to_string(),
        };
    }

    if needs_signer_seeds && uses_token_program_account {
        return Risk {
            level: RiskLevel::High,
            reason: "Signer seeds and a token_program account are present — PDA authority and program-account switching both need manual confirmation.".to_string(),
        };
    }

    if uses_token_program_account {
        return Risk {
            level: RiskLevel::Medium,
            reason: "Token program account is referenced — it must be made switchable during the rollout window.".to_string(),
        };
    }

    if is_checked {
        return Risk {
            level: RiskLevel::Medium,
            reason: "Checked variant — mint decimal parity must be validated against the new program.".to_string(),
        };
    }

    Risk {
        level: RiskLevel::Low,
        reason: "Plain CPI call site with no obvious authority or program-account constraints.".to_string(),
    }
}
