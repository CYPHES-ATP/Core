//! The stratum bridge end to end, with a CPU miner standing in for a GPU.
//!
//! A regtest node pays a fresh `cyphes-wallet` address. The `cyphes-stratum`
//! binary serves the node's templates, and a stratum client that follows
//! Beam's `miner_client.cpp` (login, job, solve, check the job difficulty,
//! submit) mines blocks through it with the reference BeamHash III solver.
//! The test checks that the blocks land on the chain, that the wallet
//! receives their rewards, and that the bridge refuses bad solutions.
//!
//! With `BEAM3REF` pointing at the Beam reference harness
//! (`cyphes-pow/reference/build.sh`), every accepted solution is also checked
//! by Beam's own C++ BeamHash III code, which GPU miners implement.
//!
//! ```sh
//! cargo build -p zebrad --features internal-miner
//! cargo test -p cyphes-stratum --test stratum_bridge -- --ignored --nocapture
//! ```

use std::{
    fs,
    io::{BufRead, BufReader, Read, Write},
    net::TcpStream,
    path::{Path, PathBuf},
    process::{Child, Command, Stdio},
    time::{Duration, Instant},
};

use cyphes_pow::BeamDifficulty;
use secrecy::SecretVec;
use serde_json::{json, Value};

const COIN: u64 = 100_000_000;
const BLOCKS: usize = 3;

fn workspace() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .expect("workspace")
        .to_owned()
}

struct Process(Child);

impl Drop for Process {
    fn drop(&mut self) {
        let _ = self.0.kill();
        let _ = self.0.wait();
    }
}

fn rpc(port: u16, method: &str, params: Value) -> Result<Value, String> {
    let body = json!({ "jsonrpc": "2.0", "id": 1, "method": method, "params": params }).to_string();
    let mut stream = TcpStream::connect(("127.0.0.1", port)).map_err(|e| e.to_string())?;
    write!(
        stream,
        "POST / HTTP/1.1\r\nHost: 127.0.0.1\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
        body.len()
    )
    .map_err(|e| e.to_string())?;
    let mut response = String::new();
    stream
        .read_to_string(&mut response)
        .map_err(|e| e.to_string())?;
    let (head, body) = response
        .split_once("\r\n\r\n")
        .ok_or("malformed response")?;
    let body = if head
        .to_ascii_lowercase()
        .contains("transfer-encoding: chunked")
    {
        dechunk(body)
    } else {
        body.to_owned()
    };
    let value: Value = serde_json::from_str(&body).map_err(|e| format!("{e}: {body}"))?;
    match value.get("error") {
        Some(error) if !error.is_null() => Err(error.to_string()),
        _ => Ok(value["result"].clone()),
    }
}

fn dechunk(body: &str) -> String {
    let mut out = String::new();
    let mut rest = body;
    while let Some((size, tail)) = rest.split_once("\r\n") {
        let size = usize::from_str_radix(size.trim(), 16).unwrap_or(0);
        if size == 0 {
            break;
        }
        out.push_str(&tail[..size]);
        rest = &tail[size + 2..];
    }
    out
}

fn start_node(dir: &Path, rpc_port: u16, miner_address: &str) -> Process {
    let config = format!(
        r#"[mining]
internal_miner = false
miner_address = "{miner_address}"

[network]
network = "Regtest"
listen_addr = "127.0.0.1:{p2p}"
initial_mainnet_peers = []
initial_testnet_peers = []
cache_dir = false

[rpc]
listen_addr = "127.0.0.1:{rpc_port}"
lightwalletd_listen_addr = "127.0.0.1:{grpc}"
enable_cookie_auth = false

[state]
cache_dir = "{state}"
ephemeral = false

[tracing]
use_color = false
"#,
        p2p = rpc_port - 1,
        grpc = rpc_port + 1,
        state = dir.join("state").display(),
    );
    fs::write(dir.join("zebrad.toml"), config).expect("node config");
    let zebrad = workspace().join("target/debug/zebrad");
    assert!(
        zebrad.exists(),
        "build the node first: cargo build -p zebrad --features internal-miner"
    );
    let log = fs::File::create(dir.join("zebrad.log")).expect("node log");
    let child = Command::new(zebrad)
        .arg("-c")
        .arg(dir.join("zebrad.toml"))
        .arg("start")
        .stdout(log.try_clone().expect("log"))
        .stderr(log)
        .spawn()
        .expect("start zebrad");
    let deadline = Instant::now() + Duration::from_secs(120);
    while rpc(rpc_port, "getblockcount", json!([])).is_err() {
        assert!(Instant::now() < deadline, "node did not start");
        std::thread::sleep(Duration::from_millis(500));
    }
    std::thread::sleep(Duration::from_secs(2));
    Process(child)
}

/// A stratum client doing what Beam's `miner_client.cpp` does, with the CPU
/// solver in place of a GPU.
struct Miner {
    stream: TcpStream,
    reader: BufReader<TcpStream>,
    job: Option<Value>,
}

impl Miner {
    fn connect(port: u16) -> Miner {
        let deadline = Instant::now() + Duration::from_secs(30);
        let stream = loop {
            match TcpStream::connect(("127.0.0.1", port)) {
                Ok(stream) => break stream,
                Err(_) if Instant::now() < deadline => {
                    std::thread::sleep(Duration::from_millis(200))
                }
                Err(e) => panic!("connect to the bridge: {e}"),
            }
        };
        let reader = BufReader::new(stream.try_clone().expect("clone"));
        Miner {
            stream,
            reader,
            job: None,
        }
    }

    fn send(&mut self, message: Value) {
        writeln!(self.stream, "{message}").expect("send");
    }

    /// Reads messages until a `result` for `id`, keeping the newest job.
    fn result(&mut self, id: &str) -> Value {
        loop {
            let mut line = String::new();
            assert!(
                self.reader.read_line(&mut line).expect("read") > 0,
                "bridge hung up"
            );
            let message: Value = serde_json::from_str(&line).expect("json line");
            match message["method"].as_str() {
                Some("job") => self.job = Some(message),
                Some("result") if message["id"] == json!(id) => return message,
                _ => {}
            }
        }
    }

    /// The newest job, waiting for one whose id is not `stale`.
    fn job_other_than(&mut self, stale: Option<&str>) -> Value {
        while self
            .job
            .as_ref()
            .is_none_or(|job| stale.is_some_and(|id| job["id"] == json!(id)))
        {
            let mut line = String::new();
            assert!(
                self.reader.read_line(&mut line).expect("read") > 0,
                "bridge hung up"
            );
            let message: Value = serde_json::from_str(&line).expect("json line");
            if message["method"] == json!("job") {
                self.job = Some(message);
            }
        }
        self.job.clone().expect("job")
    }

    fn solution(&mut self, job_id: &str, nonce: &[u8; 8], solution: &[u8; 104]) -> Value {
        self.send(json!({
            "jsonrpc": "2.0",
            "id": job_id,
            "method": "solution",
            "nonce": hex::encode(nonce),
            "output": hex::encode(solution),
        }));
        self.result(job_id)
    }
}

/// Checks `(input, nonce, solution)` with Beam's C++ BeamHash III, if the
/// reference harness is available.
fn beam_verifies(input: &str, nonce: &str, solution: &str) -> Option<bool> {
    let harness = std::env::var_os("BEAM3REF")?;
    let mut child = Command::new(harness)
        .arg("verify")
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .spawn()
        .expect("run beam3ref");
    writeln!(
        child.stdin.take().expect("stdin"),
        "{input} {nonce} {solution}"
    )
    .expect("write");
    let output = child.wait_with_output().expect("beam3ref output");
    Some(String::from_utf8_lossy(&output.stdout).trim() == "1")
}

#[test]
#[ignore = "slow: mines real BeamHash III blocks through the bridge; see the module docs"]
fn gpu_miners_protocol_mines_blocks_that_pay_the_wallet() {
    let root = workspace().join("target/stratum-bridge");
    let _ = fs::remove_dir_all(&root);
    fs::create_dir_all(&root).expect("test dir");
    let (rpc_port, stratum_port) = (26_975, 26_977);
    let runtime = tokio::runtime::Runtime::new().expect("runtime");

    // A fresh wallet. Creating it needs a running node, so start one paying
    // a throwaway address, then restart it paying the wallet.
    let node_dir = root.join("node");
    fs::create_dir_all(&node_dir).expect("node dir");
    const PLACEHOLDER: &str = "cyphregtest1gymj99nyn4c9n68u3vcuqlr57p8evnukeqfnt5l3tvnq0na2amv6tz270dwfnzrg8x63yhvskqgalaewgnqy6zts0842s3w2esyrtye0";
    let node = start_node(&node_dir, rpc_port, PLACEHOLDER);
    let server = format!("http://127.0.0.1:{}", rpc_port + 1);
    let seed = SecretVec::new(rand::random::<[u8; 32]>().to_vec());
    let (mut wallet, address) = runtime.block_on(async {
        let mut client = cyphes_wallet::connect(&server)
            .await
            .expect("connect wallet");
        let mut wallet = cyphes_wallet::Wallet::create(
            &root.join("wallet.db"),
            cyphes_wallet::Network::Regtest,
            &seed,
            1,
            &mut client,
        )
        .await
        .expect("create wallet");
        wallet.set_coinbase_confirmations(1).expect("policy");
        let address = wallet.address().expect("address");
        (wallet, address)
    });
    drop(node);
    let _node = start_node(&node_dir, rpc_port, &address);
    println!("node pays {address}");

    let _bridge = Process(
        Command::new(env!("CARGO_BIN_EXE_cyphes-stratum"))
            .args(["--node", &format!("http://127.0.0.1:{rpc_port}")])
            .args(["--listen", &format!("127.0.0.1:{stratum_port}")])
            .stdout(fs::File::create(root.join("bridge.log")).expect("log"))
            .stderr(fs::File::create(root.join("bridge.err")).expect("log"))
            .spawn()
            .expect("start the bridge"),
    );

    let mut miner = Miner::connect(stratum_port);
    miner.send(json!({ "jsonrpc": "2.0", "id": "login", "method": "login", "api_key": "rig1" }));
    let login = miner.result("login");
    assert_eq!(login["code"], json!(0), "{login}");
    assert_eq!(
        (login["forkheight"].clone(), login["forkheight2"].clone()),
        (json!(0), json!(0))
    );

    let mut found = Vec::new();
    let mut checked_bad = false;
    let mut stale: Option<String> = None;
    while found.len() < BLOCKS {
        let job = miner.job_other_than(stale.as_deref());
        let (job_id, input_hex) = (
            job["id"].as_str().expect("id").to_owned(),
            job["input"].as_str().expect("input").to_owned(),
        );
        let input = hex::decode(&input_hex).expect("input hex");
        let difficulty = BeamDifficulty(job["difficulty"].as_u64().expect("difficulty") as u32);
        let nonce: [u8; 8] = rand::random();
        for solution in cyphes_pow::solver::solve(&input, &nonce, [0; 4]) {
            // Like a real miner: submit only what meets the job difficulty.
            if !difficulty.is_target_reached(&cyphes_pow::solution_hash(&solution)) {
                continue;
            }
            if !checked_bad {
                let mut bad = solution;
                bad[3] ^= 1;
                let reply = miner.solution(&job_id, &nonce, &bad);
                assert_eq!(
                    reply["code"],
                    json!(2),
                    "a corrupted solution is rejected: {reply}"
                );
                let reply = miner.solution("no-such-job", &nonce, &solution);
                assert_eq!(
                    reply["code"],
                    json!(3),
                    "an unknown job is expired: {reply}"
                );
                checked_bad = true;
            }
            // The job difficulty is the block target, so this is a block.
            let reply = miner.solution(&job_id, &nonce, &solution);
            assert_eq!(
                reply["code"],
                json!(1),
                "a solution meeting the job difficulty is accepted: {reply}"
            );
            let hash = reply["blockhash"].as_str().expect("a block").to_owned();
            if let Some(ok) = beam_verifies(&input_hex, &hex::encode(nonce), &hex::encode(solution))
            {
                assert!(ok, "Beam's own verifier accepts the solution");
            }
            println!("block {hash} at height {}", job["height"]);
            found.push((hash, job["height"].as_u64().expect("height")));
            // A repeat never counts twice: refused as a duplicate, or as
            // expired once the new tip has replaced the job.
            let reply = miner.solution(&job_id, &nonce, &solution);
            assert!(
                reply["code"] == json!(2) || reply["code"] == json!(3),
                "{reply}"
            );
            stale = Some(job_id.clone());
            break;
        }
        // Otherwise no solution from this nonce met the difficulty: try another.
    }

    // The blocks are on the chain, at consecutive heights.
    for (hash, height) in &found {
        let block = rpc(rpc_port, "getblock", json!([hash, 1])).expect("block on chain");
        assert_eq!(block["height"].as_u64(), Some(*height), "{hash}");
    }
    let heights: Vec<u64> = found.iter().map(|(_, h)| *h).collect();
    assert_eq!(heights, (1..=BLOCKS as u64).collect::<Vec<_>>());

    // The wallet receives every reward.
    let balance = runtime.block_on(async {
        let mut client = cyphes_wallet::connect(&server)
            .await
            .expect("connect wallet");
        wallet.sync(&mut client).await.expect("sync");
        wallet
            .balance(cyphes_wallet::policy(1, 1).expect("policy"))
            .expect("balance")
    });
    println!("wallet: {balance:?}");
    assert_eq!(balance.total, BLOCKS as u64 * 1_000 * COIN, "{balance:?}");
    if std::env::var_os("BEAM3REF").is_none() {
        println!("BEAM3REF not set: solutions were not re-checked with Beam's C++ code");
    }
}
