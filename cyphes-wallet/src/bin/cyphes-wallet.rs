//! `cyphes-wallet`: a CASH wallet on the command line.
//!
//! A wallet is a directory holding `wallet.db` (the wallet database),
//! `seed.txt` (the BIP-39 recovery phrase, readable only by its owner) and
//! `wallet.json` (network and birthday). Every command prints JSON.
//!
//! ```sh
//! cyphes-wallet --wallet alice init --network regtest
//! cyphes-wallet --wallet alice sync
//! cyphes-wallet --wallet alice balance
//! cyphes-wallet --wallet alice send --to cyphregtest1... --amount 1500
//! ```
//!
//! `seed.txt` is the money: anyone who reads it can spend. This CLI is for
//! development and testing; the desktop app keeps the seed in the OS keychain.

use std::{
    fs,
    path::{Path, PathBuf},
};

use bip0039::{Count, English, Mnemonic};
use clap::{Parser, Subcommand};
use cyphes_wallet::{format_cash, parse_cash, policy, Network, Wallet};
use secrecy::SecretVec;
use serde_json::{json, Value};

#[derive(Parser)]
#[command(name = "cyphes-wallet", about = "CASH wallet (Ironwood-only)")]
struct Cli {
    /// Wallet directory.
    #[arg(long, default_value = "cash-wallet")]
    wallet: PathBuf,

    /// Node light-wallet gRPC endpoint (default: the network's local port).
    #[arg(long)]
    server: Option<String>,

    /// Confirmations before coinbase notes can be spent (wallet policy).
    #[arg(long, default_value_t = cyphes_params::WALLET_COINBASE_CONFIRMATIONS)]
    coinbase_confirmations: u32,

    #[command(subcommand)]
    command: Command,
}

#[derive(Subcommand)]
enum Command {
    /// Create a new wallet with a fresh recovery phrase.
    Init {
        #[arg(long)]
        network: Network,
        /// Height of the first block the wallet can receive in (default: next block).
        #[arg(long)]
        birthday: Option<u32>,
    },
    /// Recreate a wallet from its recovery phrase and birthday.
    Restore {
        #[arg(long)]
        network: Network,
        /// File containing the 24-word recovery phrase.
        #[arg(long)]
        phrase_file: PathBuf,
        #[arg(long)]
        birthday: u32,
    },
    /// Print the wallet's Ironwood address.
    Address,
    /// Scan the chain up to the node's tip.
    Sync,
    /// Print balances.
    Balance {
        #[command(flatten)]
        confirmations: Confirmations,
    },
    /// Pay CASH to an address.
    Send {
        #[arg(long)]
        to: String,
        /// Amount in CASH, e.g. 1500 or 0.5.
        #[arg(long)]
        amount: String,
        #[arg(long)]
        memo: Option<String>,
        #[command(flatten)]
        confirmations: Confirmations,
        /// Build and store the transaction, print it, but do not broadcast.
        #[arg(long)]
        no_broadcast: bool,
    },
    /// List the wallet's transactions.
    History,
}

/// ZIP 315 confirmation policy.
#[derive(clap::Args)]
struct Confirmations {
    /// Confirmations for the wallet's own change.
    #[arg(long, default_value_t = 3)]
    trusted: u32,
    /// Confirmations for everything received.
    #[arg(long, default_value_t = 10)]
    untrusted: u32,
}

#[derive(serde::Serialize, serde::Deserialize)]
struct Meta {
    network: String,
    birthday: u32,
}

fn db_path(dir: &Path) -> PathBuf {
    dir.join("wallet.db")
}

fn seed_path(dir: &Path) -> PathBuf {
    dir.join("seed.txt")
}

fn meta_path(dir: &Path) -> PathBuf {
    dir.join("wallet.json")
}

fn seed_from_phrase(phrase: &str) -> Result<SecretVec<u8>, String> {
    let mnemonic = Mnemonic::<English>::from_phrase(phrase.trim())
        .map_err(|e| format!("recovery phrase: {e}"))?;
    Ok(SecretVec::new(mnemonic.to_seed("").to_vec()))
}

fn read_meta(dir: &Path) -> Result<(Network, u32), String> {
    let meta: Meta = serde_json::from_str(
        &fs::read_to_string(meta_path(dir))
            .map_err(|e| format!("no wallet in {}: {e}", dir.display()))?,
    )
    .map_err(|e| format!("wallet.json: {e}"))?;
    Ok((meta.network.parse()?, meta.birthday))
}

fn write_private(path: &Path, contents: &str) -> Result<(), String> {
    use std::io::Write;
    #[cfg(unix)]
    use std::os::unix::fs::OpenOptionsExt;

    let mut options = fs::OpenOptions::new();
    options.write(true).create_new(true);
    #[cfg(unix)]
    options.mode(0o600);
    let mut file = options
        .open(path)
        .map_err(|e| format!("{}: {e}", path.display()))?;
    file.write_all(contents.as_bytes())
        .map_err(|e| format!("{}: {e}", path.display()))
}

async fn create(
    dir: &Path,
    network: Network,
    phrase: &str,
    birthday: Option<u32>,
    server: &str,
) -> Result<Value, String> {
    let seed = seed_from_phrase(phrase)?;
    let mut client = cyphes_wallet::connect(server)
        .await
        .map_err(|e| e.to_string())?;
    let birthday = match birthday {
        Some(b) => b,
        None => {
            cyphes_wallet::tip_height(&mut client)
                .await
                .map_err(|e| e.to_string())?
                + 1
        }
    };
    fs::create_dir_all(dir).map_err(|e| format!("{}: {e}", dir.display()))?;
    let mut wallet = Wallet::create(&db_path(dir), network, &seed, birthday, &mut client)
        .await
        .map_err(|e| e.to_string())?;
    write_private(&seed_path(dir), &format!("{phrase}\n"))?;
    fs::write(
        meta_path(dir),
        serde_json::to_string_pretty(&Meta {
            network: network.to_string(),
            birthday,
        })
        .expect("serializable"),
    )
    .map_err(|e| e.to_string())?;
    Ok(json!({
        "network": network.to_string(),
        "birthday": birthday,
        "address": wallet.address().map_err(|e| e.to_string())?,
        "seed_file": seed_path(dir),
    }))
}

async fn run(cli: Cli) -> Result<Value, String> {
    let dir = cli.wallet.as_path();
    match cli.command {
        Command::Init { network, birthday } => {
            let phrase = Mnemonic::<English>::generate(Count::Words24)
                .phrase()
                .to_string();
            let server = cli.server.unwrap_or_else(|| network.default_server());
            create(dir, network, &phrase, birthday, &server).await
        }
        Command::Restore {
            network,
            phrase_file,
            birthday,
        } => {
            let phrase = fs::read_to_string(&phrase_file)
                .map_err(|e| format!("{}: {e}", phrase_file.display()))?;
            let server = cli.server.unwrap_or_else(|| network.default_server());
            create(dir, network, phrase.trim(), Some(birthday), &server).await
        }
        command => {
            let (network, birthday) = read_meta(dir)?;
            let server = cli.server.unwrap_or_else(|| network.default_server());
            let mut wallet = Wallet::open(&db_path(dir), network).map_err(|e| e.to_string())?;
            wallet
                .set_coinbase_confirmations(cli.coinbase_confirmations)
                .map_err(|e| e.to_string())?;
            match command {
                Command::Address => {
                    Ok(json!({ "address": wallet.address().map_err(|e| e.to_string())? }))
                }
                Command::Sync => {
                    let mut client = cyphes_wallet::connect(&server)
                        .await
                        .map_err(|e| e.to_string())?;
                    wallet.sync(&mut client).await.map_err(|e| e.to_string())?;
                    let balance = wallet.balance(policy(3, 10).map_err(|e| e.to_string())?);
                    Ok(json!({ "synced": true, "birthday": birthday, "balance": balance.ok() }))
                }
                Command::Balance { confirmations } => {
                    let b = wallet
                        .balance(
                            policy(confirmations.trusted, confirmations.untrusted)
                                .map_err(|e| e.to_string())?,
                        )
                        .map_err(|e| e.to_string())?;
                    Ok(json!({
                        "total": format_cash(b.total),
                        "spendable": format_cash(b.spendable),
                        "pending": format_cash(b.pending),
                        "coinbase_locked": format_cash(b.coinbase_locked),
                        "base_units": b,
                    }))
                }
                Command::Send {
                    to,
                    amount,
                    memo,
                    confirmations,
                    no_broadcast,
                } => {
                    let phrase =
                        fs::read_to_string(seed_path(dir)).map_err(|e| format!("seed: {e}"))?;
                    let seed = seed_from_phrase(&phrase)?;
                    let amount = parse_cash(&amount).map_err(|e| e.to_string())?;
                    let built = wallet
                        .build_payment(
                            &seed,
                            &to,
                            amount,
                            memo.as_deref(),
                            policy(confirmations.trusted, confirmations.untrusted)
                                .map_err(|e| e.to_string())?,
                        )
                        .map_err(|e| e.to_string())?;
                    if !no_broadcast {
                        let mut client = cyphes_wallet::connect(&server)
                            .await
                            .map_err(|e| e.to_string())?;
                        cyphes_wallet::broadcast(&mut client, &built.raw)
                            .await
                            .map_err(|e| e.to_string())?;
                    }
                    Ok(json!({
                        "txid": built.txid.to_string(),
                        "amount": format_cash(amount.into_u64()),
                        "fee": format_cash(built.fee),
                        "broadcast": !no_broadcast,
                        "raw": hex::encode(&built.raw),
                    }))
                }
                Command::History => {
                    Ok(json!({ "transactions": wallet.history().map_err(|e| e.to_string())? }))
                }
                Command::Init { .. } | Command::Restore { .. } => unreachable!("handled above"),
            }
        }
    }
}

#[tokio::main]
async fn main() {
    let cli = Cli::parse();
    match run(cli).await {
        Ok(value) => println!(
            "{}",
            serde_json::to_string_pretty(&value).expect("serializable")
        ),
        Err(e) => {
            println!("{}", json!({ "error": e }));
            std::process::exit(1);
        }
    }
}
