# librustzcash fork

CYPHES patches exactly one librustzcash crate: `zcash_protocol`, which holds
every network constant the other librustzcash crates read (address and key
prefixes, coin type, consensus branch IDs, activation heights, `MAX_MONEY`).
`zcash_address`, `zcash_keys`, `zcash_primitives` and the wallet crates are
used unmodified from crates.io; they pick up the CYPHES values through this
crate. `orchard` is never patched.

| Crate | Upstream version | Upstream commit |
|---|---|---|
| `zcash_protocol` | 0.10.6 (crates.io) | `28cf1143f932dae94d8626fc0d26a6c61b08823c` (`components/zcash_protocol`) |

The first commit adding a crate here is its unmodified crates.io source; the
CYPHES changes are separate commits on top, so `git log -p librustzcash/` is
the whole fork. The root `Cargo.toml` applies it with `[patch.crates-io]`,
and so must any wallet built on librustzcash.

## Keeping the patch applied

`[patch.crates-io]` only replaces versions that satisfy every dependent's
version requirement. If a dependency ever needs a newer `zcash_protocol` than
the one vendored here, Cargo silently resolves the registry version instead
and the CYPHES constants disappear (21 million `MAX_MONEY`, Zcash's branch ID
and `u1…` addresses). So:

- keep the vendored crate at the newest release of its minor series;
- watch for `warning: Patch ... was not used in the crate graph`;
- the `parameters::cyphes_consistency` tests in `zebra-chain`, and the
  equivalent test in `cyphes-wallet`, fail if the patch is not in effect.

`src/zip318.rs` (added upstream in 0.10.6) is Zcash's Orchard-to-Ironwood
wallet migration policy. CYPHES has no Orchard pool to migrate from, so it is
unused.
