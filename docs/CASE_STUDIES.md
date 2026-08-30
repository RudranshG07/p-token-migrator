# Real-world scans

The analyzer has been validated against three architecturally distinct public
Solana protocols. Each surfaced a different mix of patterns; the scanner now
handles all three correctly.

Numbers below are from `sta scan <project>/src --out manifest.json --summary`
against `main` at the time of writing (May 2026). Re-running may produce
slightly different totals as the upstream protocols evolve.

---

## marginfi-v2 (lending)

Repo: <https://github.com/mrgnlabs/marginfi-v2>
Architectural style: Anchor, multi-program (marginfi + Kamino/JupLend/Drift
adapter modules), heavy use of `anchor_spl::token_interface` plus
`anchor_spl::token_2022` for direct Token-2022 calls.

| Metric | Value |
|---|---|
| Files scanned | 114 |
| Call sites detected | **74** |
| Direct (`token_interface`) | 14 |
| Direct (`anchor_cpi`) | 3 |
| Direct (`spl_instruction` via `spl_token_2022::onchain`) | 2 |
| Wrapped (cross-file helpers) | 55 |
| High risk findings | 57 |
| Aggregate CU saved (if migrated) | 368,520 |
| Estimated savings | 95.8% |

Top wrappers detected (cross-file resolution):

| Callers | Wrapper |
|---|---|
| 11 | `withdraw_spl_transfer` |
| 6 | `cpi_transfer_user_to_liquidity_vault` |
| 3 | `deposit_spl_transfer` |
| 2 | `cpi_transfer_obligation_owner_to_destination` |
| 2 | `cpi_transfer_user_to_obligation_owner` |
| 2 | `cpi_transfer_liquidity_vault_to_destination` |

Each of these is a domain-specific helper that wraps a CPI; every caller
inherits the operation and risk classification of its target.

**Bugs surfaced by this scan and fixed in the scanner:**

1. Single-segment bare-name calls — `use anchor_spl::token_interface::{transfer_checked, ...}`
   then `transfer_checked(cpi_ctx, amount, decimals)` was rejected by a
   premature `segments.len() < 2` guard. Scanner now matches via the alias
   map for single-segment paths too.
2. `anchor_spl::token_2022` was missing from the recognized paths. Added
   alongside `anchor_spl::token` and `anchor_spl::token_interface`.
3. Wrapper-name collision filter — `setup` is defined in both the fuzz
   harness and a wrapper helper, contaminating the wrapper map. The fix:
   drop wrapper entries whose name has multiple definitions of which only
   some transitively touch a CPI.
4. `spl_token_2022::onchain::invoke_transfer_checked` — a lower-level
   helper that builds and invokes a Transfer with transfer-hook resolution.
   Hit at `state/bank.rs:660,707` was missed twice: once because the path
   wasn't in the recognized set, and once because the re-export
   `use anchor_spl::token_2022::spl_token_2022;` made the expanded path
   `anchor_spl::token_2022::spl_token_2022::onchain::invoke_transfer_checked`
   — longer than the exact match expected. Fix: match on the *last two
   segments* of the path prefix (robust to re-export aliasing).

---

## drift-v2 (perps)

Repo: <https://github.com/drift-labs/protocol-v2>
Architectural style: Anchor. Fully migrated to `anchor_spl::token_interface`
to support both legacy SPL Token and Token-2022 mints. Custom
`transfer_checked_with_transfer_hook` wrapper for Token-2022 transfer hooks.

| Metric | Value |
|---|---|
| Files scanned | 167 |
| Call sites detected | **74** |
| Direct (`token_interface`) | 8 |
| Wrapped (cross-file helpers) | 66 |
| High risk findings | 74 (all Token-2022 interface-aware) |
| Aggregate CU saved (if migrated) | 376,700 |
| Estimated savings | 95.7% |

Top wrappers:

| Callers | Wrapper | Where defined |
|---|---|---|
| 24 | `receive` | `controller/token.rs:96` |
| 17 | `send_from_program_vault` | `controller/token.rs` |
| 8 | `attempt_settle_revenue_to_insurance_fund` | controller |
| 7 | `send_from_program_vault_with_signature_seeds` | controller |
| 2 | `execute_token_transfer` | controller |
| 2 | `transfer_from_program_vault` | controller |

Drift uses `receive` as the domain term for "incoming token transfer." The
single definition at `controller/token.rs:96` calls
`transfer_checked_with_transfer_hook` or `token_interface::transfer_checked`
depending on the mint's hook configuration — the wrapper map correctly
propagates the resulting `Transfer` operation to every one of the 24
callers.

Every Drift finding is tagged high-risk because the `token_interface` /
Token-2022 path needs transfer-hook, memo-extension, and confidential-transfer
validation end-to-end.

---

## phoenix-v1 (DEX)

Repo: <https://github.com/Ellipsis-Labs/phoenix-v1>
Architectural style: native Solana program (no Anchor). Uses
`spl_token::instruction::*` directly. Owned `src/program/token_utils.rs`
helper module for transfer plumbing.

| Metric | Value |
|---|---|
| Files scanned | 52 |
| Call sites detected | **88** |
| Direct (`spl_instruction`) | 4 |
| Wrapped (cross-file helpers) | 84 |
| Operations | transfer (46), mint_to (40), initialize_account (2) |
| Aggregate CU saved (if migrated) | 476,760 |
| Estimated savings | 95.7% |

The IDL JSON at `idl/phoenix_v1.json` was correctly flagged with a
`legacy_token_program_reference` hint (it embeds the
`TokenkegQfeZyiNwAJbNbGKPFXCWuBvf9Ss623VQ5DA` address).

The 40 `mint_to` findings concentrate in `tests/test_phoenix.rs` because
the test harness mints tokens for setup. Production-only counts (filtered
to `src/program/`) are much lower; the test-side findings are valid but
not migration-relevant for protocol teams.

---

## What the three together prove

| Pattern | Validated against |
|---|---|
| Anchor + `anchor_spl::token` direct CPI | marginfi |
| Anchor + `anchor_spl::token_interface` direct CPI | marginfi, drift |
| Anchor + `anchor_spl::token_2022` direct CPI | marginfi |
| Native + `spl_token::instruction` direct CPI | phoenix |
| Cross-file helper-function wrapping a CPI | all three |
| Multi-level transitive wrappers (helper → helper → CPI) | all three |
| Methods on `impl` blocks called via `self.method(...)` | drift |
| IDL JSON referencing the legacy SPL Token program | phoenix |
| Token-2022 transfer-hook awareness via `token_interface` | drift |

These three protocols cover the range of token-CPI patterns currently in
production on Solana. The analyzer's findings are reproducible by anyone
cloning the repos and running the same command.

---

## Reproducing these scans

```bash
# Install
cargo install --git https://github.com/RudranshG07/p-token-migrator --bin sta --locked

# marginfi
git clone --depth 1 https://github.com/mrgnlabs/marginfi-v2.git
sta scan marginfi-v2/programs/marginfi/src --summary

# drift
git clone --depth 1 https://github.com/drift-labs/protocol-v2.git
sta scan protocol-v2/programs/drift/src --summary

# phoenix
git clone --depth 1 https://github.com/Ellipsis-Labs/phoenix-v1.git
sta scan phoenix-v1 --summary
```
