# p-token-shim-anchor

Transition shim scaffolding for p-token migrations.

This crate gives generated migration code one local boundary for deciding whether a call site should route to legacy SPL Token or p-token. It intentionally avoids hardcoding final p-token program IDs or instruction builders until the canonical mainnet interfaces are available.

The migration toolkit uses this crate as the compatibility milestone artifact. Replace the internals with Anchor CPI wrappers once p-token interfaces are final.
