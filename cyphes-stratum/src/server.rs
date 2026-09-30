//! The bridge: long-polls the node for block templates, serves them to
//! miners as stratum jobs, and submits the blocks miners find.

use std::{
    collections::VecDeque,
    net::SocketAddr,
    sync::{
        atomic::{AtomicU64, Ordering},
        Arc, Mutex,
    },
    time::Duration,
};

use futures::StreamExt;
use rand::Rng;
use serde_json::{json, Value};
use tokio::{
    io::AsyncWriteExt,
    net::{TcpListener, TcpStream},
    sync::watch,
};
use tokio_util::codec::{FramedRead, LinesCodec};
use zebra_chain::serialization::ZcashSerialize;

use crate::{
    job::{Job, SharedJob, Template, Verdict},
    node::Node,
    protocol::{self, Code, Request},
};

/// Jobs kept for the current chain tip. Solutions to any of them still make
/// a valid block; older ones are answered `expired`.
const RECENT_JOBS: usize = 8;

/// The longest line a miner may send.
const MAX_LINE: usize = 16 * 1024;

/// Bridge settings.
#[derive(Clone, Debug)]
pub struct Config {
    /// Raw Beam difficulty to ask miners for; `None` asks for blocks only.
    pub share_difficulty: Option<u64>,
    /// Hex digits of nonce prefix assigned to each connection (0 to 6), so
    /// rigs never search the same nonces; 0 disables it.
    pub nonce_prefix_digits: usize,
}

/// Counters, for logs.
#[derive(Default, Debug)]
pub struct Stats {
    pub shares: AtomicU64,
    pub rejected: AtomicU64,
    pub blocks: AtomicU64,
}

/// A stratum bridge to one node.
pub struct Bridge {
    node: Node,
    config: Config,
    /// Recent jobs for the current tip, newest last.
    jobs: Mutex<VecDeque<SharedJob>>,
    current: watch::Sender<Option<SharedJob>>,
    next_job_id: AtomicU64,
    pub stats: Stats,
}

impl Bridge {
    pub fn new(node: Node, config: Config) -> Arc<Bridge> {
        assert!(config.nonce_prefix_digits <= 6, "at most 6 prefix digits");
        Arc::new(Bridge {
            node,
            config,
            jobs: Mutex::new(VecDeque::new()),
            current: watch::channel(None).0,
            next_job_id: AtomicU64::new(1),
            stats: Stats::default(),
        })
    }

    /// Keeps the current job up to date: long-polls `getblocktemplate` and
    /// publishes a job whenever the template changes.
    pub async fn run_templates(self: Arc<Self>) {
        let mut long_poll_id: Option<String> = None;
        loop {
            let params = match &long_poll_id {
                Some(id) => json!([{ "longpollid": id }]),
                None => json!([]),
            };
            let template: Template = match self.node.call("getblocktemplate", params).await {
                Ok(template) => template,
                Err(e) => {
                    tracing::warn!("no block template: {e}");
                    long_poll_id = None;
                    tokio::time::sleep(Duration::from_secs(2)).await;
                    continue;
                }
            };
            long_poll_id = Some(template.long_poll_id.clone());
            let id = self.next_job_id.fetch_add(1, Ordering::Relaxed).to_string();
            match Job::from_template(id, &template, self.config.share_difficulty) {
                Ok(job) => self.publish(job, template.submit_old == Some(false)),
                Err(e) => {
                    tracing::error!("{e}");
                    tokio::time::sleep(Duration::from_secs(2)).await;
                }
            }
        }
    }

    /// Makes `job` the current job. Earlier jobs stay valid unless the node
    /// says old work is stale or the job builds on a different block.
    pub fn publish(&self, job: Job, old_work_is_stale: bool) {
        let job = Arc::new(job);
        {
            let mut jobs = self.jobs.lock().expect("unpoisoned");
            let new_tip = jobs
                .back()
                .is_some_and(|last| last.previous_block_hash != job.previous_block_hash);
            if old_work_is_stale || new_tip {
                jobs.clear();
            }
            jobs.push_back(job.clone());
            while jobs.len() > RECENT_JOBS {
                jobs.pop_front();
            }
        }
        tracing::info!(
            job = %job.id,
            height = job.height,
            difficulty = format_args!("{:#010x}", job.share_difficulty.0),
            "new job"
        );
        self.current.send_replace(Some(job));
    }

    fn job(&self, id: &str) -> Option<SharedJob> {
        let jobs = self.jobs.lock().expect("unpoisoned");
        jobs.iter().find(|job| job.id == id).cloned()
    }

    /// Accepts miners on `listener` until the task is dropped.
    pub async fn serve(self: Arc<Self>, listener: TcpListener) {
        loop {
            match listener.accept().await {
                Ok((stream, peer)) => {
                    tokio::spawn(self.clone().handle(stream, peer));
                }
                Err(e) => {
                    tracing::warn!("accept: {e}");
                    tokio::time::sleep(Duration::from_millis(100)).await;
                }
            }
        }
    }

    /// One miner connection.
    async fn handle(self: Arc<Self>, stream: TcpStream, peer: SocketAddr) {
        tracing::info!(%peer, "miner connected");
        let (read, mut write) = stream.into_split();
        let mut lines = FramedRead::new(read, LinesCodec::new_with_max_length(MAX_LINE));
        let mut jobs = self.current.subscribe();
        let mut logged_in = false;
        let nonce_prefix = self.nonce_prefix();

        loop {
            let reply = tokio::select! {
                line = lines.next() => {
                    let line = match line {
                        Some(Ok(line)) => line,
                        Some(Err(e)) => {
                            tracing::info!(%peer, "dropping miner: {e}");
                            break;
                        }
                        None => break,
                    };
                    if line.trim().is_empty() {
                        continue;
                    }
                    match protocol::parse(&line) {
                        Ok(Request::Login { id, .. }) => {
                            logged_in = true;
                            let mut reply = protocol::login_result(&id, Code::Success, &nonce_prefix);
                            if let Some(job) = jobs.borrow_and_update().clone() {
                                reply.push_str(&job_message(&job));
                            }
                            reply
                        }
                        Ok(Request::Solution { id, nonce, output }) if logged_in => {
                            self.solution(&id, &nonce, &output, &nonce_prefix, peer).await
                        }
                        Ok(Request::Solution { id, .. }) => protocol::result(&id, Code::LoginFailed),
                        Ok(Request::Other { method, .. }) => {
                            tracing::debug!(%peer, %method, "ignoring method");
                            continue;
                        }
                        Err(Code::UnknownMethod) => {
                            tracing::debug!(%peer, "ignoring unknown method");
                            continue;
                        }
                        Err(code) => {
                            // Beam's server drops a peer that sends malformed messages.
                            tracing::info!(%peer, "dropping miner: {}", code.description());
                            break;
                        }
                    }
                }
                changed = jobs.changed(), if logged_in => {
                    if changed.is_err() {
                        break;
                    }
                    match jobs.borrow_and_update().clone() {
                        Some(job) => job_message(&job),
                        None => continue,
                    }
                }
            };
            if write.write_all(reply.as_bytes()).await.is_err() {
                break;
            }
        }
        tracing::info!(%peer, "miner disconnected");
    }

    fn nonce_prefix(&self) -> String {
        let digits = self.config.nonce_prefix_digits;
        let random: [u8; 3] = rand::thread_rng().gen();
        hex::encode(random)[..digits].to_owned()
    }

    /// Checks a solution and, if it is a block, submits it. Returns the
    /// `result` line for the miner.
    async fn solution(
        &self,
        id: &str,
        nonce: &str,
        output: &str,
        nonce_prefix: &str,
        peer: SocketAddr,
    ) -> String {
        let Some(job) = self.job(id) else {
            return protocol::result(id, Code::Expired);
        };
        let verdict = {
            let (nonce, output, prefix) =
                (nonce.to_owned(), output.to_owned(), nonce_prefix.to_owned());
            let job = job.clone();
            tokio::task::spawn_blocking(move || job.check(&nonce, &output, &prefix))
                .await
                .unwrap_or(Verdict::Rejected("verifier task failed"))
        };
        match verdict {
            Verdict::Rejected(reason) => {
                self.stats.rejected.fetch_add(1, Ordering::Relaxed);
                tracing::info!(%peer, job = %id, "rejected solution: {reason}");
                protocol::result(id, Code::Rejected)
            }
            Verdict::Share => {
                self.stats.shares.fetch_add(1, Ordering::Relaxed);
                tracing::debug!(%peer, job = %id, "share");
                protocol::result(id, Code::Accepted)
            }
            Verdict::Block(block) => self.submit_block(id, &job, *block, peer).await,
        }
    }

    async fn submit_block(
        &self,
        id: &str,
        job: &Job,
        block: zebra_chain::block::Block,
        peer: SocketAddr,
    ) -> String {
        let hash = block.hash();
        let Ok(bytes) = block.zcash_serialize_to_vec() else {
            return protocol::result(id, Code::Rejected);
        };
        match self
            .node
            .call::<Value>("submitblock", json!([hex::encode(bytes)]))
            .await
        {
            Ok(Value::Null) => {
                self.stats.blocks.fetch_add(1, Ordering::Relaxed);
                tracing::info!(%peer, job = %id, height = job.height, %hash, "block accepted");
                protocol::block_found(id, &hash.to_string())
            }
            Ok(response) => {
                // Usually a race with another block at this height.
                let stale = self.job(id).is_none();
                tracing::warn!(%peer, job = %id, height = job.height, %hash, %response, "node refused block");
                protocol::result(id, if stale { Code::Expired } else { Code::Rejected })
            }
            Err(e) => {
                tracing::error!(%peer, job = %id, %hash, "submitblock failed: {e}");
                protocol::result(id, Code::Rejected)
            }
        }
    }
}

fn job_message(job: &Job) -> String {
    protocol::job(&job.id, &job.input, job.share_difficulty.0, job.height)
}
