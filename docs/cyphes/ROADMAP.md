# CYPHES v1 roadmap

What is done, and the ordered work to a mainnet launch. Protocol rules are in
[SPEC.md](SPEC.md). The currency is **CASH**; the network is CYPHES.

The order is deliberate: prove the money works from a command line before
building GPU mining on it, and prove both before putting it in the desktop app.

## Done

- `cyphes-pow`: BeamHash III verifier and solver, differential-tested
  against Beam's own C++ code.
- `cyphes-params`: emission, LWMA-1 difficulty, network identity.
- A 220-byte BeamHash III block header replaces Equihash.
- CASH subsidy, 10 billion `MAX_MONEY`, 25-second blocks; network magic and
  ports; activation list `{0: Genesis, 1: NU6.3}`; Ironwood-only coinbase;
  genesis builder and embedded genesis blocks.
- Consensus rule `ironwood_only`: no transparent, Sprout, Sapling or legacy
  Orchard transactions after genesis.
- LWMA-1 difficulty; 3-minute future time limit; the 10-minute
  median-time-past bound enforced from genesis on every network.
- Storage isolated from Zebra: data under `<OS cache>/cyphes`, config
  `cyphes.toml`, no Zcash seeders.
- Regtest validates proof of work.
- librustzcash fork (`librustzcash/zcash_protocol`): 10 billion `MAX_MONEY`,
  CYPHES branch ID `0x4535c5e0`, activation heights, address prefixes
  (`cyph1…`) and coin type, pinned to `cyphes-params` by a consistency test.

- CYPHES block vectors mined under the fork; the devnet mines to a
  `cyphregtest1…` address.
- `cyphes-wallet`: a CASH wallet library and CLI on `zcash_client_backend`
  0.24 and `zcash_client_sqlite` 0.22 (see `cyphes-wallet/README.md`).
- **The CLI money cycle is proven** by `cyphes-wallet/tests/money_cycle.rs`
  against real nodes, every block mined with BeamHash III: mine and receive
  under the 100-confirmation coinbase policy; corrupted proof of work
  rejected by the node, each for its specific reason; spend with exact
  reconciliation; restore from seed; crash recovery (SIGKILL, and a torn
  block backup); a two-node reorganisation that orphans a payment the
  receiver could already spend, with an unknown-anchor rejection; double
  spends rejected in the mempool and in blocks mined with valid proof of
  work; broken value conservation rejected; a mined 22,000,000 CASH note
  received and spent. `tests/large_value_fixture.rs` adds a synthetic
  9,999,000,000 CASH Ironwood note, created and then spent, each proven and
  verified.
- Node durability: the non-finalized block backup is written on every
  commit, atomically (temporary file, fsync, rename). A node killed without
  warning restarts with its newest blocks; a torn backup file is skipped,
  costing only that block.
- Wallet coinbase policy: mainnet wallets cannot lower the 100-confirmation
  policy.

## 1. Remaining parameter and test work

- Rewrite the Zebra tests that assert Zcash parameters, and delete the ones
  for removed features (`TESTS-PENDING-VECTORS.md`).
- A fresh full workspace build and Clippy run.
- Wallet: `zip321` payment URIs still use the `zcash:` scheme.

## 2. Money cycle follow-ups

- Run `money_cycle` in CI on every change to the node, the fork or the
  wallet (it needs about 30 minutes and 10 GiB), plus the fast CYPHES
  suites on every push.
- Durability is proven for process crashes (SIGKILL). Power loss is covered
  by fsync, but untested: it needs a VM or fault-injecting filesystem.
- Zebra retries transactions submitted over RPC. A transaction rejected for
  an unknown anchor can therefore confirm later, if a reorganisation makes
  that anchor known. The harness observed this; it is correct, but wallets
  should expect it.

## 3. GPU mining

A stratum bridge (`cyphes-stratum`) between `getblocktemplate` /
`submitblock` and Beam's stratum protocol: the job `input` is the header's
PoW input, miners vary the 8-byte nonce, share difficulty uses Beam's packed
format, and the bridge re-verifies each share before submitting. Done when
lolMiner or GMiner on a real GPU mines blocks that the network accepts and the
wallet from step 2 receives the payouts.

## 4. Wallet in the CYPHES desktop app

Integrate the proven wallet from step 2 into the Tauri app
(`CYPHES-ATP/Node`), as a client of the node:

- keys and signing in a separate component from the cybersecurity agents,
  which never get access to the seed or signing;
- seed in the OS keychain; one ZIP 32 Ironwood account; `cyph1…` addresses;
- Wallet tab (balance, receive, send, history) and Mine tab;
- for v1, ATP credits neither settle to nor redeem from CASH.

## 5. Subtraction

Consensus already rejects these; this removes the code, crate by crate, with
the CYPHES vectors as the safety net:

- transparent: `zebra-script` and the C++ `zcash_script` dependency, the UTXO
  set, transparent RPCs and address indexes;
- Sprout and Sapling: note commitment trees, Groth16 verification and its
  parameters, RPC fields;
- funding-stream and lockbox machinery; Zcash RPC compatibility;
- rename the `zebrad` binary to `cyphesd`.

## 6. Launch checklist

1. External audit of the diff against Zebra v6.4.2 and of the librustzcash
   fork diff.
2. Public testnet with independent GPU miners for at least one LWMA window
   under hash-rental swings.
3. Decide the open parameters (SPEC section 9), including the hash-rental
   defence.
4. Publish the launch time and the Bitcoin block the genesis message will
   commit to; mine the mainnet genesis at the launch difficulty estimate;
   replace the provisional genesis; tag the release.
5. Seed nodes and DNS seeders under CYPHES infrastructure.

## Devnet

```sh
cargo build -p zebrad --features internal-miner
./target/debug/zebrad -c devnet/regtest.toml start
```

The reference CPU miner uses every core and about 8 GiB of RAM.
