# CYPHES v1 protocol

Private money. Mined into existence by anyone.

CYPHES is a new layer 1: Zcash's Ironwood shielded pool on a Zebra-derived
Rust node, with BeamHash III GPU proof of work, a fixed 10 billion supply, and
no transparent ledger. This document is the source of truth for the rules.
Every number here is defined in code in `cyphes-params` (shared by node and
wallet) or `cyphes-pow`, and each section names where.

Status on 2026-09-30: regtest runs end to end; testnet and mainnet parameters
are set, but the mainnet genesis block is **provisional** (see Launch).

## 1. Money

| Rule | Value | Code |
|---|---|---|
| Ticker, base unit | CASH, 10^-8 CASH | `cyphes_params::COIN` |
| Block subsidy | 1,000 CASH, genesis pays 0 | `cyphes_params::block_subsidy` |
| Halving | every 5,000,000 blocks (about 3.96 years); block `k * 5,000,000` starts era `k` | `HALVING_INTERVAL` |
| Total issuance | 9,999,998,999.25 CASH, strictly below 10 billion; last subsidy at block 184,999,999 | `TOTAL_ISSUANCE` |
| Amount cap | 10 billion CASH (`MAX_MONEY`) | `cyphes_params::MAX_MONEY` |
| Premine, founders' reward, dev fund, lockbox, slow start | none | `zebra-chain` subsidy functions |
| Transaction fees | ZIP 317, paid to the miner | unchanged from Zebra |
| Coinbase maturity | **none in consensus**; wallets wait 100 confirmations by policy | `WALLET_COINBASE_CONFIRMATIONS` |

Proof of work is the only way CASH is created. Nothing in CYPHES CORE (the AI
audit network) can mint, and no model score or verifier vote changes the
subsidy.

"No fee" in the launch notes means no protocol fee: no founders' reward and no
dev fund. Transaction fees stay, because Halo 2 verification costs real CPU and
a zero-fee shielded chain is free to spam. The fee level is an open parameter
(section 9).

## 2. Ledger: Ironwood only

The shielded protocol is Zcash's Ironwood pool (NU6.3), used byte for byte from
the `orchard` crate (0.15.x): Halo 2 with no trusted setup, Pallas/Vesta, the
post-NU6.3 action circuit fixed after the June 2026 counterfeiting bug, V3
quantum-recoverable note plaintexts (ZIP 2005). CYPHES never modifies the
circuit crate. `orchard` 0.13.x (the vulnerable circuit) is yanked upstream and
must never be depended on.

Consensus rule `ironwood_only` (`zebra-consensus/src/transaction/check.rs`),
for every transaction after genesis, in blocks and in the mempool:

- version MUST be 6;
- no transparent outputs, no Sprout JoinSplits, no Sapling spends or outputs,
  no Orchard actions;
- the only transparent input allowed is a coinbase's input, which carries the
  block height and no value.

Coinbase pays the miner through Ironwood outputs only, encrypted to the
all-zero outgoing viewing key (ZIP 213). A miner address without an
Orchard-shaped receiver is a configuration error.

### Coinbase contract

- **Coinbase is public by design.** Anyone can decrypt every coinbase output
  with the zero outgoing viewing key: its recipient address and amount are
  public, which is what makes issuance auditable. Privacy comes from ordinary
  shielded payments afterwards. Miners who want address privacy should mine to
  a dedicated address and move the funds on.
- **There is no consensus coinbase maturity.** Zcash's 100-block rule applies
  only to transparent coinbase outputs (ZIP 213), and CYPHES has none. A
  shielded spend does not reveal which note it spends, so consensus cannot
  apply a maturity rule. If a reorg removes a coinbase, the anchors and
  nullifiers of transactions that spent it no longer exist on the new chain,
  and those transactions become invalid rather than creating value.
- **Wallet policy.** Wallets treat coinbase notes as spendable only after
  `WALLET_COINBASE_CONFIRMATIONS` (100) confirmations, and ordinary received
  notes after a smaller confirmation depth. This is client policy, so it can
  change without a network upgrade. `cyphes-wallet` implements it by locking
  each coinbase note (a coinbase is always its block's first transaction)
  until `mined height + 99`, using librustzcash's input locks, so it becomes
  spendable exactly at 100 confirmations.

### Light wallets

Wallets sync from the node's light-wallet gRPC server (lightwalletd's
`CompactTxStreamer`, built into the node). Because every upgrade activates at
block 1, a wallet born at block 1 reads the genesis state, so the server
reads a block ID with neither hash nor height as genesis. Upstream Zebra and
lightwalletd reject that request.

### Supply auditability

Issuance is public (zero-OVK coinbase) and Zebra tracks the Ironwood pool
balance (ZIP 209). But with no transparent pool there is nowhere for value to
leave the shielded pool, so the turnstile that bounds a counterfeiting bug in
Zcash has nothing to guard until value moves. CYPHES's answer is **pool
epochs**: a future upgrade opens a new pool and migrates value across a
turnstile, as Zcash did from Orchard to Ironwood (ZIP 318). Migration cannot
exceed the recorded pool balance, so counterfeit value is detected at the
epoch boundary. v1 ships epoch 1 (Ironwood); the epoch mechanism is a v2 item.

## 3. Proof of work: BeamHash III

`cyphes-pow`, used by `zebra-chain/src/work/beamhash.rs`.

| Field | Size | Meaning |
|---|---|---|
| header prefix | 108 bytes | version, previous hash, merkle root, commitments, time, bits |
| nonce | 8 bytes | Beam nonce |
| solution | 104 bytes | 32 packed 25-bit indices, then a 4-byte extra nonce |

The header is a fixed 220 bytes on every network (Zcash's is 1,487).

- PoW input = BLAKE2b-256(personal `CYPHES_PowHeader`, header prefix). This is
  the 32-byte `input` a Beam stratum job hands a miner.
- Valid iff `solution` is a BeamHash III solution for `(input, nonce)` (Blake2b
  personalised `Beam-PoW‖448‖5`, SipHash-2-4 leaves, five Wagner rounds on
  24-bit collisions) **and** `SHA-256(solution)`, read big-endian, is at most
  the target in `bits`. This is Beam's rule, so lolMiner and GMiner filter
  shares correctly without changes.
- Block hash = SHA-256d(header), as in Zcash.

Verification: the Rust verifier is differential-tested against Beam's own C++
reference (`cyphes-pow/reference`): 13,920 verdicts agree, and both solvers find
the identical solution set for the same input. The Rust reference solver takes
about 7 s and 8 GiB per nonce on an M5 (Beam's C++ solver: 339 s, 15.7 GiB).

**Risk:** BeamHash III hashrate is rentable (NiceHash BeamV3). A young chain
can be 51%-attacked cheaply. Mitigations before mainnet are in section 9.

## 4. Difficulty: LWMA-1

`cyphes_params::lwma_next_target`, used by `zebra-state` in
`service/check/difficulty.rs`.

- Target spacing 25 s; window N = 120 blocks (about 50 minutes).
- Solvetimes use monotonic timestamps (`t_j = max(time_j, t_{j-1} + 1)`), each
  capped at 6T; weights 1..=N, newest heaviest; 512-bit arithmetic, so there is
  no overflow and no early truncation.
- `next = (sum targets) * (sum j * st_j) / (n * n(n+1)/2 * T)`, clamped to
  `[1, pow_limit]`, over `n = min(N, blocks since genesis)`.
- Block 1 keeps the genesis target. Genesis `bits` is therefore a launch
  parameter: an estimate of launch hashrate, erring hard. Simulated in
  `cyphes-params`: a 10x-too-hard start recovers in 2 blocks (about 6 minutes);
  a 4x-too-easy start mines about 27 blocks ahead of schedule; starting at the
  PoW limit mines about 86 ahead (an 86,000 CASH "instamine").
- Timestamps: node-local future limit 3 minutes (Zcash: 2 hours);
  consensus limit 10 minutes after median-time-past (Zcash: 90 minutes),
  enforced from genesis on every network.
- Regtest does not retarget, but it validates proof of work like every other
  network. On regtest only, `generate` and `generatetoaddress` mine real
  BeamHash III blocks (with the `internal-miner` feature).
- Regtest only: `[network.testnet_parameters] block_subsidy = N` replaces the
  emission with a fixed N CASH per block, so tests can mine amounts above 21
  million quickly. Configuring it on any other network is a startup error.

PoW limits (compact): mainnet and testnet `0x2000ffff` (one valid solution in
256), regtest `0x207fffff`.

## 5. Network identity

`cyphes_params::network`. Magic bytes are the first four bytes of
`SHA-256("CYPHES <network>")`.

| | Mainnet | Testnet | Regtest |
|---|---|---|---|
| Magic | `05 05 29 bb` | `76 ca 23 8e` | `d4 4a 72 44` |
| P2P / RPC / wallet gRPC | 2974 / 2975 / 2976 | 12974 / 12975 / 12976 | 22974 / 22975 / 22976 |
| Unified address HRP | `cyph` | `cyphtest` | `cyphregtest` |
| UFVK / UIVK HRP | `cyphview` / `cyphivk` | `…test` | `…regtest` |
| ZIP 32 coin type | provisional `0x43415348`, must be registered in SLIP-0044 | 1 | 1 |

Consensus branch ID for the v1 rules: `0x4535c5e0`
(`SHA-256("CYPHES consensus branch v1")`), distinct from every Zcash branch ID.
It is the branch ID of the NU6.3 rule set in the node and in the librustzcash
fork, so it goes into every transaction header and ZIP 244 signature hash: no
signature is valid on both chains, and a Zcash transaction does not even
parse. (An Ironwood spend's anchor also exists on only one chain.)

Addresses, keys and prefixes come from the librustzcash fork
(`librustzcash/zcash_protocol`), which every CYPHES node and wallet must use.
Every Zcash HRP and Base58 prefix has a distinct CYPHES value, so no Zcash
address or key parses as a CYPHES one. A test in `zebra-chain`
(`parameters::cyphes_consistency`) fails if the fork and `cyphes-params` ever
disagree.

Upgrade history: the activation list is `{0: Genesis, 1: NU6.3}` on every
network. Every Zcash upgrade through NU6.3 is in force from block 1.

Default peers: none. CYPHES nodes never dial Zcash seeders.

Local data: everything lives under `<OS cache dir>/cyphes` (state databases,
peer cache, RPC cookie) and the default config file is `cyphes.toml`, so a
CYPHES node never reads or deletes a Zcash node's `zebra` directory or
`zebrad.toml` on the same machine.

## 6. Genesis

Built by `zebra-chain/examples/cyphes_genesis.rs`, embedded in
`zebra-chain/src/block/genesis/`, committed directly at startup on every
network (a new chain's first node has no peers to download it from).

- One version 1 coinbase with no outputs (genesis pays nothing). Its script is
  one 64-byte push: `CYPHES 2026-09-30 Private money. Mined into existence by anyone.`
- Mined with BeamHash III at the network's genesis target.

| Network | Hash | Time | Bits |
|---|---|---|---|
| Regtest | `5627d4e9…1b53` | 2026-09-30T00:00Z | `207fffff` |
| Testnet | `657e5d51…029b` | 2026-09-30T00:00Z | `2000ffff` |
| Mainnet | `1e0a6479…1da9` **provisional** | 2026-10-01T00:00Z | `2000ffff` |

## 7. What was deleted, kept, added

| | |
|---|---|
| **Deleted** | Equihash; Zcash genesis blocks and checkpoints (25,253 lines); founders' reward, funding-stream and lockbox constants; slow start; DigiShield difficulty and the testnet minimum-difficulty rule; the temporary Orchard soft fork; Zcash DNS seeders; transparent, Sprout, Sapling and legacy-Orchard transactions (by consensus rule; code removal is ongoing) |
| **Kept** | Ironwood / Halo 2 / Pallas-Vesta (`orchard` 0.15.x, untouched); note commitments, nullifiers, viewing keys; Zebra's state, verification pipeline, P2P and the lightwalletd-compatible gRPC server |
| **Added** | BeamHash III; LWMA-1; the CYPHES emission; new genesis, magic, ports, HRPs, branch ID; `cyphes-params` and `cyphes-pow` |

## 8. Known gaps (v1 is not done)

1. **Money cycle not yet proven end to end.** The librustzcash fork is in
   place (10 billion `MAX_MONEY`, CYPHES branch ID and prefixes), but no wallet
   has yet received, spent, restored and rescanned CASH on a live chain.
2. **Code removal.** Transparent, Sprout and Sapling are rejected by consensus
   but their code still compiles in. Removing it also removes the C++
   `zcash_script` dependency and Sapling's Groth16 parameters.
3. **Test vectors.** Zebra's Zcash block vectors no longer parse; see
   `TESTS-PENDING-VECTORS.md`.
4. **GPU mining.** A stratum bridge from `getblocktemplate` to Beam stratum,
   for lolMiner and GMiner.
5. **Wallet.** `zcash_client_backend` in the CYPHES Tauri app, syncing from the
   node's gRPC server.

## 9. Open parameters before mainnet

- Launch difficulty estimate (genesis `bits`) and launch time.
- Genesis message committing to a recent Bitcoin block hash.
- SLIP-0044 coin type registration.
- Fee level (ZIP 317 marginal fee in base units).
- Hash-rental defence: options are a rolling finality depth, checkpoint
  releases, or merge-mining with Beam. Each has trade-offs and none is chosen.
