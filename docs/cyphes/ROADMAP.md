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

A regtest devnet mines BeamHash III blocks whose coinbase pays 1,000 CASH to
an Ironwood output, with every other pool at zero.

## 1. Parameters and storage: finish and prove

- Regenerate the CYPHES block vectors under the new branch ID and prefixes.
- A node-level negative test: `submitblock` with a corrupted BeamHash III
  solution is rejected, on regtest (which now validates proof of work).
- Replace the devnet's Zcash-encoded miner address with a `cyphregtest1…`
  address.
- Rewrite the Zebra tests that assert Zcash parameters, and delete the ones
  for removed features (`TESTS-PENDING-VECTORS.md`).
- A fresh full workspace build and Clippy run.

## 2. Prove a CLI money cycle

A command-line wallet (`cyphes-wallet`) on `zcash_client_backend` and
`zcash_client_sqlite` (features `orchard`, `lightwalletd-tonic`; not
`transparent-inputs`), using the fork, syncing from the node's
lightwalletd-compatible gRPC server. Prove, on a local chain:

- mine to the wallet, then receive, wait out the wallet's coinbase policy
  (100 confirmations), and spend;
- restore from the seed and rescan to the same balance and history;
- an amount above 21 million CASH in one note;
- invalid spends rejected: double spend, wrong anchor, overspend;
- a reorganisation that orphans a received payment, and the wallet's
  recovery from it.

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
