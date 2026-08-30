use crate::manifest::Operation;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct OperationCu {
    pub legacy_cu: u64,
    pub p_token_cu: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct BenchmarkProfile {
    pub name: String,
    pub status: String,
    pub note: String,
    pub operations: HashMap<String, OperationCu>,
}

impl BenchmarkProfile {
    pub fn lookup(&self, op: Operation) -> OperationCu {
        self.operations
            .get(op.as_str())
            .cloned()
            .unwrap_or(OperationCu { legacy_cu: 0, p_token_cu: 0 })
    }
}

/// Hard-coded estimator profile. Replaced by a measured profile (step 2 of
/// the roadmap — LiteSVM harness) before this product is shipped to anyone
/// making real upgrade decisions.
pub fn default_profile() -> BenchmarkProfile {
    let pairs = [
        ("transfer", 5_200u64, 220u64),
        ("mint_to", 6_100, 260),
        ("burn", 5_700, 250),
        ("approve", 4_800, 210),
        ("close_account", 5_000, 240),
        ("initialize_account", 7_400, 360),
    ];
    let operations = pairs
        .into_iter()
        .map(|(k, l, p)| (k.to_string(), OperationCu { legacy_cu: l, p_token_cu: p }))
        .collect();
    BenchmarkProfile {
        name: "simd-0266-estimator".to_string(),
        status: "pre-mainnet-estimate".to_string(),
        note: "CU estimates are bundled placeholders until p-token interfaces and a measured profile are available.".to_string(),
        operations,
    }
}
