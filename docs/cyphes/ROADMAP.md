# CYPHES v1 roadmap

What is done, and the ordered work to a mainnet launch. Protocol rules are in
[SPEC.md](SPEC.md).

## Done (branch `cyphes/main`)

| Commit | Milestone |
|---|---|
| `d322c7757` | `cyphes-pow` (BeamHash III verifier + solver, differential-tested against Beam's C++) and `cyphes-params` (emission, LWMA, network identity) |
| `0ab0ebaae` | 220-byte BeamHash III block header replaces Equihash |
| `5827cd6e9` | CYPHES subsidy, 10B `MAX_MONEY`, 25 s spacing, magic/ports, `{0: Genesis, 1: NU6.3}`, Ironwood-only coinbase, genesis builder |
| `c5213f18c` | `ironwood_only` consensus rule; embedded testnet and provisional mainnet genesis; first CYPHES block vectors |
| `55b5faf60` | LWMA-1 difficulty; 3-minute future time limit |

Regtest devnet, verified: the node commits the embedded genesis, the internal
miner mines BeamHash III blocks, each coinbase is a v6 transaction with one
Ironwood output and no transparent outputs, and after 3 blocks the Ironwood
pool holds exactly 3,000 CASH with every other pool at zero.

Run it:

```sh
cargo build -p zebrad --features internal-miner
./target/debug/zebrad -c devnet/regtest.toml start
```

with `devnet/regtest.toml` as in section 7.

## 1. librustzcash fork (next; blocks the wallet and testnet)

Fork `zcash/librustzcash` to `CYPHES-ATP/librustzcash`, pin the crate versions
this Zebra uses (`zcash_protocol` 0.10, `zcash_primitives` 0.30, `zcash_address`
0.13, `zcash_keys` 0.16, `zcash_client_backend` 0.24, `zcash_client_sqlite`
0.22), and apply it to both the node and the wallet with `[patch.crates-io]`.
Keep `orchard` unpatched.

- `zcash_protocol`: `MAX_MONEY` = 10 billion CASH (import `cyphes-params`);
  a CYPHES branch ID (`0x4535c5e0`) used for NU6.3-rules transactions; activation
  heights `{NU6.3: 1}` for Main and Test; CYPHES HRPs and coin type in
  `constants::{mainnet,testnet,regtest}`.
- `zcash_address`: unified address, UFVK and UIVK HRPs `cyph`, `cyphview`,
  `cyphivk` (and `test` / `regtest` suffixes). Reject Sapling, Sprout and
  transparent encodings for CYPHES networks.
- Then in Zebra: switch `ConsensusBranchId` for NU6.3 to the CYPHES value,
  remove the ZEC-named units from RPC output, and re-enable the two proptests
  listed in `TESTS-PENDING-VECTORS.md`.

Done when: a `cyphregtest1…` address mines on regtest, and a test transfers
more than 21 million CASH in one note.

## 2. Wallet in the CYPHES Tauri app (`CYPHES-ATP/Node`)

Build the product around the wallet, not the protocol around the wallet code:
the wallet is a client of the node's gRPC server and shares only
`cyphes-params` with it.

- `src-tauri/src/wallet/`: `zcash_client_backend` (features `orchard`,
  `lightwalletd-tonic`; **not** `transparent-inputs`) and `zcash_client_sqlite`,
  from the fork. Network parameters from `cyphes-params`.
- Keys: BIP-39 seed in the OS keychain; one ZIP 32 Orchard account; address
  is an Ironwood-only unified address.
- Sync: `CompactTxStreamer` against the node's `rpc.lightwalletd_listen_addr`
  (Zebra 6.4's server already emits Ironwood actions and tree sizes).
- Tauri commands: `wallet_create`, `wallet_restore`, `wallet_address`,
  `wallet_balance`, `wallet_send`, `wallet_history`, `wallet_sync_status`.
- React: a Wallet tab (balance, receive QR, send, history) and a Mine tab
  (local devnet miner, or lolMiner settings for the stratum bridge).
- For v1, ATP credits neither settle to nor redeem from CASH.

Done when: the app receives a coinbase from a local node, waits out maturity,
and sends to a second wallet, all Ironwood.

## 3. GPU mining: stratum bridge

A small service (`cyphes-stratum`) between `getblocktemplate` /
`submitblock` and Beam's stratum protocol:

- job `input` = `Solution::pow_input(header)`; miners vary the 8-byte nonce;
- share difficulty in Beam's packed format (`cyphes_pow::BeamDifficulty`);
  the bridge re-verifies each share (`verify_solution` plus the target) before
  `submitblock`;
- test against lolMiner and GMiner on testnet.

## 4. Subtraction

Consensus already rejects these; this removes the code:

- transparent: `zebra-script` and the C++ `zcash_script` dependency, the UTXO
  set, transparent RPCs and address-index state;
- Sprout and Sapling: note commitment trees, Groth16 verification and its
  parameters, Sapling RPC fields;
- funding-stream and lockbox machinery still left in `zebra-chain` parameters;
- Zcash RPC compatibility (`zcashd_compat`).

Do it crate by crate, with the CYPHES vectors (section 5) as the safety net.

## 5. Test migration

Replace the Zcash block vectors in `zebra-test` with CYPHES chains:
a longer regtest chain, a testnet chain with LWMA retargeting, and chains
with Ironwood spends (not only coinbase). Delete tests for removed features.
Track progress in `TESTS-PENDING-VECTORS.md`.

## 6. Launch checklist

1. External audit of the diff against Zebra v6.4.2 (the PoW, subsidy, LWMA
   and `ironwood_only` changes are small and reviewable).
2. Public testnet with independent GPU miners for at least one LWMA window
   under hash-rental swings.
3. Decide the open parameters (SPEC section 9), including the hash-rental
   defence.
4. Publish launch time and the Bitcoin block the genesis message will commit
   to; mine the mainnet genesis at the launch difficulty estimate; replace the
   provisional genesis; tag the release.
5. Seed nodes and DNS seeders under CYPHES infrastructure.

## 7. Devnet config

```toml
[mining]
internal_miner = true
# Orchard-only unified address; Zcash regtest encoding until section 1.
miner_address = "uregtest1pszqlgxaf5w8mu2yd9uygg8cswp0ec4f7eejqnqc35tztw4tk0sxnt3pym2f3s2872cy2ruuc5n8y9cen5q6ngzlmzu8ztrjesv8zm9j"

[network]
network = "Regtest"
listen_addr = "127.0.0.1:22974"
cache_dir = false

[rpc]
listen_addr = "127.0.0.1:22975"
enable_cookie_auth = false

[state]
ephemeral = true
```
