//! Forked-mainnet replay infrastructure.
//!
//! The architecture in three layers:
//!
//! 1. [`rpc`] — thin synchronous JSON-RPC client for the Solana cluster
//!    endpoints we need (`getSignaturesForAddress`, `getTransaction`,
//!    `getMultipleAccounts`, `getAccountInfo`). Synchronous because the
//!    replay workflow is inherently serial per transaction; an async
//!    runtime would add weight without buying parallelism.
//!
//! 2. [`fork`] — given a list of pubkeys we want to "fork", fetches the
//!    accounts from the cluster, then installs them into a [`litesvm::LiteSVM`]
//!    instance. The forked state is the substrate on which replay runs.
//!
//! 3. [`replay`] — for each tx, snapshots all referenced accounts into a
//!    fresh LiteSVM, builds a comparable tx with a synthetic fee payer, and
//!    records the outcome alongside mainnet's recorded outcome. The
//!    [`report`] module renders the diff to JSON.
//!
//! Honest limitations of this first cut (documented in the emitted report):
//! - Signers other than the fee payer cannot be impersonated; programs that
//!   gate on a non-payer signer will fail in replay.
//! - Account state is fetched at the latest finalized slot, not the slot the
//!   tx was originally executed at — historical state isn't queryable on
//!   public RPC. State drift between the two is surfaced as a divergence.

pub mod alt;
pub mod fork;
pub mod loader;
pub mod replay;
pub mod report;
pub mod rpc;

pub use alt::{decode_lookup_table, resolve_account_list, ResolvedAccount};
pub use fork::Fork;
pub use replay::{
    compare_builds, replay_signature, BuildDiffOutcome, ReplayOptions, ReplayOutcome,
};
pub use report::{BuildDiffReport, ReplayReport, ReplayReportEntry};
pub use rpc::RpcClient;
