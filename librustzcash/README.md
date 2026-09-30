# librustzcash fork

CYPHES patches exactly one librustzcash crate: `zcash_protocol`, which holds
every network constant the other librustzcash crates read (address and key
prefixes, coin type, consensus branch IDs, activation heights, `MAX_MONEY`).
`zcash_address`, `zcash_keys`, `zcash_primitives` and the wallet crates are
used unmodified from crates.io; they pick up the CYPHES values through this
crate. `orchard` is never patched.

| Crate | Upstream version | Upstream commit |
|---|---|---|
| `zcash_protocol` | 0.10.1 (crates.io) | `033a0a9b8c32d82006d67984ed145f4827ca5219` (`components/zcash_protocol`) |

The first commit adding a crate here is its unmodified crates.io source; the
CYPHES changes are separate commits on top, so `git log -p librustzcash/` is
the whole fork. The root `Cargo.toml` applies it with `[patch.crates-io]`,
and so must any wallet built on librustzcash.
