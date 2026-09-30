# cyphes-stratum

A stratum bridge between a CYPHES node and BeamHash III GPU miners.

The node builds block templates (`getblocktemplate`) and accepts blocks
(`submitblock`). GPU miners such as lolMiner and GMiner speak Beam's stratum
protocol. The bridge sits between them:

- it long-polls the node and turns each template into a stratum `job`, whose
  `input` is the header's 32-byte PoW input (the BLAKE2b hash of every header
  field before the nonce) and whose `difficulty` is the block target in
  Beam's packed format, rounded so that anything a miner submits meets it;
- miners search the 8-byte nonce and return `nonce` and the 104-byte
  solution (`output`);
- the bridge checks every solution itself, in order: nonce prefix, BeamHash
  III, share difficulty, repeats, then the block target, and submits the
  blocks to the node.

Blocks pay the node's `[mining] miner_address`. Use your wallet's `cyph1…`
address there.

## Running it

```sh
# The node, with a miner address:
#   [mining]
#   miner_address = "cyph1..."
cyphes-stratum --node http://127.0.0.1:2975 --rpc-cookie <state cache dir>/.cookie
```

It listens on `127.0.0.1:2977`. Use `--listen 0.0.0.0:2977` to accept rigs from
the local network. The bridge has no authentication: anyone who can reach
the port can mine, and every block pays the node's address. Don't expose the
port to the internet.

Miners must use BeamHash III explicitly:

```sh
lolMiner --algo BEAM-III --pool 127.0.0.1:2977 --user rig1 --tls off
miner --algo beamhashIII --server 127.0.0.1:2977 --user rig1 --ssl 0   # GMiner
```

The login reply sets `forkheight` and `forkheight2` to 0 (BeamHash III from
genesis). Miners choose between BeamHash I, II and III by height, and would
otherwise assume Beam mainnet's fork heights.

Options:

- `--share-difficulty N`: ask miners for Beam difficulty `N`, lower than the
  block's, so rigs report work between blocks. The default asks for blocks
  only.
- `--nonce-prefix-digits D`: give each connection a random `D`-hex-digit nonce
  prefix (Beam's `nonceprefix`), so several rigs never search the same nonces.
  Off by default; random 64-bit nonces rarely collide.

## What is proven

`tests/stratum_bridge.rs` runs a regtest node paying a fresh `cyphes-wallet`
address, starts the bridge binary, and mines through it with a stratum
client that follows Beam's `miner_client.cpp`: log in, take the job, solve
with the reference CPU solver, submit only what meets the job difficulty. It
checks:

- the login reply, and that the blocks land at heights 1, 2 and 3;
- that every solution the bridge accepted also passes Beam's own C++
  BeamHash III verifier (set `BEAM3REF` to the harness built by
  `cyphes-pow/reference/build.sh`);
- that the bridge rejects a corrupted solution, answers `expired` for an
  unknown job, and never counts a repeat;
- that the wallet receives every reward.

```sh
cargo build -p zebrad --features internal-miner
BEAM3REF=/path/to/beam3ref cargo test -p cyphes-stratum --test stratum_bridge -- --ignored --nocapture
```

Unit tests rebuild CYPHES block 1 exactly from its template plus a miner's
nonce and solution, check share-versus-block classification, and pin the
message formats to Beam's `stratum.cpp`. `BeamDifficulty::from_target` in
`cyphes-pow` is tested on thousands of targets: every hash its difficulty
accepts meets the target, and it gives up at most a 2^-24 fraction.

Not yet proven: a real GPU miner. lolMiner and GMiner are closed source and
run on Linux and Windows. The last step of the GPU milestone is a real rig
mining blocks through this bridge on a CYPHES devnet.

TLS is not supported yet; run the bridge next to the rigs, or tunnel it.
