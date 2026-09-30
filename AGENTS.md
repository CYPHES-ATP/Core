# CYPHES Chain: agent guidelines

This repository is the CYPHES layer 1: a fork of Zebra v6.4.2 (Zcash
Foundation, MIT/Apache-2.0) that keeps Zcash's Ironwood shielded pool and
replaces Zcash's monetary and PoW layer. Read `docs/cyphes/SPEC.md` (the rules)
and `docs/cyphes/ROADMAP.md` (what is done and next) before changing anything.

It is **not** an upstream Zebra checkout. Upstream's PR contribution gate does
not apply; do not open pull requests against `ZcashFoundation/zebra` from here.

## Hard rules

1. **Never modify the shielded circuit.** The `orchard` crate (0.15.x,
   Ironwood) is used unmodified from crates.io. Do not patch, vendor or fork
   it, and never depend on `orchard` 0.13.x (the counterfeitable circuit,
   yanked upstream). A circuit bug is a counterfeiting bug.
2. **Every CYPHES-specific rule lives in `cyphes-params` or `cyphes-pow`**,
   with tests, and Zebra code calls into them. Do not scatter CYPHES constants
   through Zebra crates.
3. **Consensus changes need a test that fails without them.** For PoW changes,
   keep `cyphes-pow`'s differential test against Beam's C++ reference passing
   (`BEAM3REF=… cargo test -p cyphes-pow --test reference`, see
   `cyphes-pow/reference/build.sh`).
4. **Proof of work is the only issuance.** Nothing may add a premine, fee
   recipient, funding stream or any other path that creates CASH.
5. **Anonymity.** This project publishes no personal information: no names,
   emails, account IDs, local paths or timezones. Commit only as the
   `atpprotocol` GitHub account with its GitHub-provided noreply address
   (check `git var GIT_AUTHOR_IDENT` before every commit), with UTC dates and
   no co-author or sign-off trailers.
6. **Never dial Zcash infrastructure.** No Zcash seeders, ports or magic bytes
   in defaults.

## Where things are

| Concern | Location |
|---|---|
| Emission, supply, LWMA, magic, ports, HRPs, branch ID | `cyphes-params/` |
| BeamHash III verifier, solver, Beam C++ harness | `cyphes-pow/` |
| Block header PoW fields and checks | `zebra-chain/src/work/beamhash.rs`, `zebra-consensus/src/block/check.rs` |
| Subsidy functions (call into `cyphes-params`) | `zebra-chain/src/parameters/network/subsidy.rs` |
| Ironwood-only transaction rule | `zebra-consensus/src/transaction/check.rs` (`ironwood_only`) |
| Difficulty adjustment | `zebra-state/src/service/check/difficulty.rs` |
| Genesis blocks and builder | `zebra-chain/src/block/genesis/`, `zebra-chain/examples/cyphes_genesis.rs` |
| CYPHES block vectors | `zebra-test/src/vectors/cyphes/` |
| Local devnet | `devnet/regtest.toml` |
| librustzcash fork (money, branch ID, addresses) | `librustzcash/zcash_protocol/` |
| Wallet library, CLI, money-cycle harness | `cyphes-wallet/` |
| Stratum bridge for GPU miners | `cyphes-stratum/` |

Many Zebra tests still parse Zcash block vectors and fail;
`docs/cyphes/TESTS-PENDING-VECTORS.md` lists them. Do not "fix" them by
restoring Zcash behaviour.

## Upstream security fixes

Zebra's security releases (for example 6.4.2's V6 transaction DoS fix) usually
apply here too. Track the `upstream` remote and cherry-pick them.

If you find a vulnerability in code inherited from Zebra, it probably affects
Zcash as well: report it privately to the Zcash Foundation (see `SECURITY.md`)
before, or alongside, fixing it here. Never publish it first.

---

The rest of this file is Zebra's engineering reference, which still applies to
the inherited crates.

## Project Structure & Module Organization

Zebra is a Rust workspace. Main crates include:

- `zebrad/` (node CLI/orchestration),
- core libraries like `zebra-chain/`, `zebra-consensus/`, `zebra-network/`, `zebra-state/`, `zebra-rpc/`,
- support crates like `zebra-node-services/`, `zebra-test/`, `zebra-utils/`, `tower-batch-control/`, and `tower-fallback/`.

Code is primarily in each crate's `src/`; integration tests are in `*/tests/`; many unit/property tests are colocated in `src/**/tests/` (for example `prop.rs`, `vectors.rs`, `preallocate.rs`). Documentation is in `book/` and `docs/decisions/`. CI and policy automation live in `.github/workflows/`.

## Build, Test, and Development Commands

All of these must pass before submitting a PR:

```bash
# Optional full build check
cargo build --workspace --locked

# All three must pass before any PR
cargo fmt --all -- --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace

# Run a single crate's tests
cargo test -p zebra-chain
cargo test -p zebra-state

# Run a single test by name
cargo test -p zebra-chain -- test_name

# CI-like nextest profile (unit + integration, excludes stateful and E2E)
cargo nextest run --profile ci --locked --release --features default-release-binaries --run-ignored=all

# Zebrad test category and GCP profile examples are maintained in zebrad/tests/main.rs.
```

## Commit & Pull Request Guidelines

- PR titles must follow [conventional commits](https://www.conventionalcommits.org/en/v1.0.0/#specification) (PRs are merged with a merge commit — the PR title becomes the merge commit message)
- Branch commits are preserved in history, so each commit message must be meaningful and follow conventional commits too — release-plz reads the commits that land on `main` when it picks the next version
- A breaking change needs the `!` marker on the PR title _and_ on the branch commit that introduces it: the PR gate reads the title, release-plz reads the commits
- Do not add `Co-Authored-By` tags for AI tools, in _any_ commit on the branch — every one of them is preserved on `main`, not just the PR title
- Do not add "Generated with [tool]" footers, in any commit on the branch
- Use `.github/pull_request_template.md` and include: motivation, solution summary, test evidence, issue link (`Closes #...`), and AI disclosure.
- For user-visible changes, update `CHANGELOG.md` per the [Changelog Guidelines](book/src/dev/changelog-guidelines.md).

## Project Overview

Zebra is a Zcash full node implementation in Rust. It is a validator node — it excludes features not strictly needed for block validation and chain sync.

- **Rust edition**: 2021
- **MSRV**: 1.88 (libraries), 1.91 (zebrad binary)
- **Database format version**: defined in `zebra-state/src/constants.rs`

## Crate Architecture

```text
zebrad (CLI orchestration)
  ├── zebra-consensus (block/transaction verification)
  │     └── zebra-script (script validation via FFI)
  ├── zebra-state (finalized + non-finalized storage)
  ├── zebra-network (P2P, peer management)
  └── zebra-rpc (JSON-RPC + gRPC)
        └── zebra-node-services (service trait aliases)
              └── zebra-chain (core data types, no async)
```

**Dependency rules**:

- Dependencies flow **downward only** — lower crates must not depend on higher ones
- `zebra-chain` is **sync-only**: no async, no tokio, no Tower services
- `zebra-node-services` defines service trait aliases used across crates
- `zebrad` orchestrates all components but contains minimal logic
- Utility crates: `tower-batch-control`, `tower-fallback`, `zebra-test`

### Per-Crate Concerns

| Crate | Key Concerns |
| --- | --- |
| `zebra-chain` | Serialization correctness, no async, consensus-critical data structures |
| `zebra-network` | Protocol correctness, peer handling, rate limiting, DoS resistance |
| `zebra-consensus` | Verification completeness, error handling, checkpoint vs semantic paths |
| `zebra-state` | Read/write separation (`ReadRequest` vs `Request`), database migrations |
| `zebra-rpc` | zcashd compatibility, error responses, timeout handling |
| `zebra-script` | FFI safety, memory management, lifetime/ownership across boundaries |

## Coding Style & Naming Conventions

- Rust 2021 conventions and `rustfmt` defaults apply across the workspace (4-space indentation).
- Naming: `snake_case` for functions/modules/files, `CamelCase` for types/traits, `SCREAMING_SNAKE_CASE` for constants.
- Respect workspace lint policy in `.cargo/config.toml` and crate-specific lint config in `clippy.toml`.
- Keep dependencies flowing downward across crates; maintain `zebra-chain` as sync-only.

## Code Patterns

### Tower Services

All services must include these bounds:

```rust
S: Service<Req, Response = Resp, Error = BoxError> + Send + Clone + 'static,
S::Future: Send + 'static,
```

- `poll_ready` must check all inner services
- Clone services before moving into async blocks

### Error Handling

- Use `thiserror` with `#[from]` / `#[source]` for error chaining
- `expect()` messages must explain **why** the invariant holds, not what happens if it fails:

  ```rust
  .expect("block hash exists because we just inserted it")  // good
  .expect("failed to get block")                            // bad
  ```

- Don't turn invariant violations into misleading `None`/default values

### Numeric Safety

- External/untrusted values: use `saturating_*` / `checked_*` arithmetic
- All `as` casts must have a comment explaining why the cast is safe

### Async & Concurrency

- CPU-heavy work (crypto, proofs): use `tokio::task::spawn_blocking`
- All external waits need timeouts (network, state, channels)
- Prefer `tokio::sync::watch` over `Mutex` for shared async state
- Prefer freshness tracking ("time since last change") to detect stalls

### Security

- Use `TrustedPreallocate` for deserializing collections from untrusted sources
- Bound all loops/allocations over attacker-controlled data
- Validate at system boundaries (network, RPC, disk)

### Performance

- Prefer existing indexed structures (maps/sets) over scanning/iterating
- Avoid unnecessary clones — structs may grow in size over time
- Use `impl Into<T>` to reduce verbose `.into()` at call sites
- Don't add unnecessary comments, docstrings, or type annotations to code you didn't change

## Testing Guidelines

- **Unit/property tests**: `src/*/tests/` within each crate (`prop.rs`, `vectors.rs`, `preallocate.rs`)
- **zebrad tests**: `zebrad/tests/main.rs` is the canonical source for test tiers and local `cargo nextest` examples.
- **Adding new zebrad tests**: Place the test in the appropriate module tier. No nextest config changes needed.
- Async tests: `#[tokio::test]` with timeouts for long-running tests
- Test configs must match real network parameters (don't rely on defaults)

```bash
# Unit tests (all crates)
cargo test --workspace

# zebrad unit + integration tests (default nextest profile excludes stateful and E2E)
cargo nextest run

# Zebrad test category and GCP profile examples are maintained in zebrad/tests/main.rs.
```

## Metrics & Observability

- Metrics use dot-separated hierarchical names with existing prefixes: `checkpoint.*`, `state.*`, `sync.*`, `rpc.*`, `peer.*`, `zcash.chain.*`
- Use `#[instrument(skip(large_arg))]` for tracing spans on important operations
- Errors must be logged with context

## Changelog

- Changelogs are generated by [changie](https://changie.dev) — never edit `CHANGELOG.md` or a crate's `CHANGELOG.md` by hand, they are regenerated from `.changes/`
- Add a change fragment instead, one `-j` per affected project: `changie new -j zebrad -k Added -b "..."`, or `changie new -j zebra-chain -j zebra-state -k breaking -b "..."` for a change spanning crates
- Kinds are `breaking` (`Breaking Changes`), `Added`, `Changed`, `Deprecated`, `Removed`, `Fixed`, `Security`
- See the [Changelog Guidelines](book/src/dev/changelog-guidelines.md) for detailed formatting rules

## Configuration

```rust
#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields, default)]
pub struct Config {
    /// Documentation for field
    pub field: Type,
}
```

- Use `#[serde(deny_unknown_fields)]` for strict validation
- Use `#[serde(default)]` for backward compatibility
- All fields must have documentation
- Defaults must be sensible for production
