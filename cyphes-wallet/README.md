# cyphes-wallet

A CASH wallet: a library with a thin command-line interface. The desktop app
is meant to use the same library.

- **Ironwood only.** It receives on, and pays only to, unified addresses with
  an Ironwood receiver (`cyph1…`, `cyphtest1…`, `cyphregtest1…`). Change goes
  to Ironwood. There is no transparent or Sapling support, and a refusing
  prover stands in for Sapling's Groth16 parameters.
- **librustzcash with the CYPHES fork.** `zcash_client_backend` 0.24 and
  `zcash_client_sqlite` 0.22, built inside the Core workspace so the
  `zcash_protocol` patch (10 billion `MAX_MONEY`, CYPHES branch ID and address
  prefixes) always applies. A unit test fails if it ever does not.
- **Syncs from a node**, over the light-wallet gRPC server built into the
  CYPHES node (`rpc.lightwalletd_listen_addr`).
- **Coinbase policy.** Mined notes are locked until 100 confirmations. Shielded
  coinbase has no consensus maturity rule, so this is wallet policy;
  `--coinbase-confirmations` changes it, for test networks only. Other
  received value follows ZIP 315 (3 confirmations for the wallet's own
  change, 10 for everything else).

## Command line

A wallet is a directory holding `wallet.db`, `seed.txt` (the 24-word recovery
phrase, readable only by its owner) and `wallet.json` (network and birthday).
Every command prints JSON.

```sh
cargo build -p cyphes-wallet
cyphes-wallet --wallet alice init --network regtest
cyphes-wallet --wallet alice address
cyphes-wallet --wallet alice sync
cyphes-wallet --wallet alice balance
cyphes-wallet --wallet alice send --to cyphregtest1... --amount 1500
cyphes-wallet --wallet alice history
cyphes-wallet --wallet alice-copy restore --network regtest --phrase-file alice/seed.txt --birthday 1
```

`--server` points at a node's wallet gRPC endpoint; the default is the
network's local port (22976 on regtest).

`seed.txt` is the money: anyone who can read it can spend. This CLI is for
development and testing.

## The money cycle

`tests/money_cycle.rs` runs the whole cycle against real local nodes, mining
every block with BeamHash III, and writes its evidence to
`target/money-cycle/REPORT.md`:

1. **mine and receive**, with the 100-confirmation coinbase policy;
2. **spend**, with balances reconciling to the base unit;
3. **restore** from seed and birthday into a fresh database;
4. **reorganisation** across two nodes, with an orphaned payment;
5. **invalid spends** sent straight to the node: double spend, unknown
   anchor, broken value conservation;
6. **above 21 million**: a mined 22,000,000 CASH note received and spent, on
   a regtest node configured with a 25 million CASH block subsidy (a
   regtest-only test setting).

It takes about 40 minutes and 10 GiB of RAM:

```sh
cargo build -p zebrad --features internal-miner
cargo test -p cyphes-wallet --test money_cycle -- --ignored --nocapture
```

`tests/large_value_fixture.rs` complements it with a synthetic Ironwood
bundle creating a single 9,999,000,000 CASH note, proven and verified with
the unmodified circuit. It proves the circuit and value types near the cap,
not anything about a chain.
