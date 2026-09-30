# CYPHES

Private money. Mined into existence by anyone.

CYPHES is a layer 1 built from Zcash's newest shielded pool, Ironwood, on a
Rust node derived from Zebra, with GPU proof of work:

```text
10B fixed supply        1,000 CASH every 25 s, halving every 5,000,000 blocks
GPU proof of work       BeamHash III (lolMiner, GMiner), LWMA-1 difficulty
Ironwood only           shielded from genesis: no transparent, Sprout or Sapling
Halo 2                  no trusted setup; circuit used unmodified
Zebra-derived node      Rust; Zcash's state, verification and P2P design
```

No premine, no founders' reward, no dev fund. Proof of work is the only way
CASH is created.

> **Status: devnet.** Regtest runs end to end: CASH is mined, received,
> spent and restored from seed by the `cyphes-wallet` CLI
> ([evidence](docs/cyphes/MONEY-CYCLE-REPORT.md)). The mainnet genesis block is
> provisional and there is no public network yet. See
> [docs/cyphes/ROADMAP.md](docs/cyphes/ROADMAP.md).

## Documents

- [Protocol spec](docs/cyphes/SPEC.md): every rule and parameter, and where it
  lives in code.
- [Roadmap](docs/cyphes/ROADMAP.md): what is done and the ordered work to
  mainnet.
- [Agent guidelines](AGENTS.md): hard rules for anyone, human or AI, changing
  this code.

## Run a devnet

Requires Rust 1.91+ and about 10 GiB of free RAM (the reference CPU miner uses
about 8 GiB).

```sh
cargo build -p zebrad --features internal-miner
./target/debug/zebrad -c devnet/regtest.toml start
```

The node commits the regtest genesis block, then mines BeamHash III blocks
whose coinbase pays 1,000 CASH to an Ironwood output. Query it on
`127.0.0.1:22975`:

```sh
curl -s -X POST -H 'content-type: application/json' \
  --data '{"jsonrpc":"2.0","id":1,"method":"getblockchaininfo","params":[]}' \
  http://127.0.0.1:22975
```

## Crates

| Crate | Purpose |
|---|---|
| `cyphes-params` | CYPHES consensus and network parameters, shared by node and wallet |
| `cyphes-pow` | BeamHash III verifier and reference solver, tested against Beam's C++ |
| `cyphes-wallet` | CASH wallet library and CLI on librustzcash; the money-cycle harness |
| `librustzcash/zcash_protocol` | the forked librustzcash crate: 10B `MAX_MONEY`, CYPHES branch ID and addresses |
| `zebra-*`, `zebrad` | the node, forked from Zebra v6.4.2 |

## Credits and license

Built on [Zebra](https://github.com/ZcashFoundation/zebra) by the Zcash
Foundation (Zebra's README is kept in
[docs/ZEBRA-README.md](docs/ZEBRA-README.md)), the
[orchard](https://github.com/zcash/orchard) crate and librustzcash by the
Electric Coin Company and Zcash developers, and BeamHash III by the
[Beam](https://github.com/BeamMW/beam) team. Dual-licensed MIT or Apache-2.0,
like Zebra.
