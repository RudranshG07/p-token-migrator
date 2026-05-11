#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TokenRoute {
    LegacySplToken,
    PToken,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PTokenOperation {
    Transfer,
    MintTo,
    Burn,
    Approve,
    CloseAccount,
    InitializeAccount,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PTokenProgramSelector<'a> {
    legacy_program_id: &'a str,
    p_token_program_id: &'a str,
    selected_program_id: &'a str,
}

impl<'a> PTokenProgramSelector<'a> {
    pub fn new(
        legacy_program_id: &'a str,
        p_token_program_id: &'a str,
        selected_program_id: &'a str,
    ) -> Self {
        Self {
            legacy_program_id,
            p_token_program_id,
            selected_program_id,
        }
    }

    pub fn route(&self, _operation: PTokenOperation) -> TokenRoute {
        if self.selected_program_id == self.p_token_program_id {
            TokenRoute::PToken
        } else {
            TokenRoute::LegacySplToken
        }
    }

    pub fn requires_review(&self) -> bool {
        self.selected_program_id != self.legacy_program_id
            && self.selected_program_id != self.p_token_program_id
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ShimPlan {
    pub route: TokenRoute,
    pub operation: PTokenOperation,
    pub checked_authority: bool,
}

impl ShimPlan {
    pub fn new(route: TokenRoute, operation: PTokenOperation, checked_authority: bool) -> Self {
        Self {
            route,
            operation,
            checked_authority,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn routes_to_p_token_program() {
        let selector = PTokenProgramSelector::new("legacy", "ptoken", "ptoken");
        assert_eq!(selector.route(PTokenOperation::Transfer), TokenRoute::PToken);
        assert!(!selector.requires_review());
    }

    #[test]
    fn defaults_unknown_programs_to_legacy_with_review() {
        let selector = PTokenProgramSelector::new("legacy", "ptoken", "other");
        assert_eq!(selector.route(PTokenOperation::Burn), TokenRoute::LegacySplToken);
        assert!(selector.requires_review());
    }
}
