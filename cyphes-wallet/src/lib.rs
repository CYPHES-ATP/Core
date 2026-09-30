//! CASH wallet: Ironwood-only, on librustzcash with the CYPHES `zcash_protocol`
//! fork.
//!
//! This library is the wallet; the `cyphes-wallet` binary is a thin CLI over
//! it, and the desktop app is meant to use it the same way. A wallet is one
//! ZIP 32 account from one seed, stored in SQLite, and it syncs from a node's
//! light-wallet gRPC server (Zebra's lightwalletd-compatible
//! `CompactTxStreamer`).
//!
//! Policy that is not consensus lives here: coinbase notes are locked until
//! [`cyphes_params::WALLET_COINBASE_CONFIRMATIONS`] confirmations (CYPHES
//! coinbase is shielded, so consensus cannot apply a maturity rule), and
//! every recipient must be an Ironwood receiver.

pub mod cache;
pub mod network;
pub mod prover;

use std::{
    fmt,
    num::NonZeroU32,
    path::{Path, PathBuf},
};

use secrecy::{ExposeSecret, SecretVec};
use tonic::transport::Channel;
use zcash_client_backend::{
    data_api::{
        chain::ChainState,
        locking::{LockOwner, OutputLockStore},
        wallet::{
            create_proposed_transactions, decrypt_and_store_transaction,
            propose_standard_transfer_to_address, ConfirmationsPolicy, SpendingKeys,
        },
        Account as _, AccountBirthday, TransactionDataRequest, TransactionStatus, WalletRead,
        WalletWrite,
    },
    fees::StandardFeeRule,
    proto::service::{
        compact_tx_streamer_client::CompactTxStreamerClient, BlockId, ChainSpec, RawTransaction,
        TxFilter,
    },
    sync,
    wallet::{OutputRef, OvkPolicy},
};
use zcash_client_sqlite::{util::SystemClock, wallet::init::init_wallet_db, AccountUuid, WalletDb};
use zcash_keys::{
    address::Address,
    keys::{UnifiedAddressRequest, UnifiedSpendingKey},
};
use zcash_primitives::{block::BlockHash, transaction::Transaction};
use zcash_protocol::{
    consensus::{BlockHeight, BranchId},
    value::Zatoshis,
    PoolType, ShieldedPool, TxId,
};

pub use cache::MemoryBlockCache;
pub use network::Network;
pub use prover::NoSapling;

/// The wallet database type.
pub type Db = WalletDb<rusqlite::Connection, Network, SystemClock, rand::rngs::OsRng>;

/// A light-wallet gRPC client connected to a node.
pub type Client = CompactTxStreamerClient<Channel>;

/// Wallet errors, as messages: the librustzcash error types are deeply
/// generic, and callers (CLI, desktop app) only need to report them.
#[derive(Debug)]
pub struct Error(pub String);

impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

impl std::error::Error for Error {}

pub type Result<T> = std::result::Result<T, Error>;

fn err(context: &str, e: impl fmt::Display) -> Error {
    Error(format!("{context}: {e}"))
}

/// Base units per CASH.
pub const COIN: u64 = cyphes_params::COIN;

/// The lock owner for coinbase notes that have not reached the wallet's
/// coinbase confirmation policy.
const COINBASE_POLICY_OWNER: LockOwner = LockOwner::new(*b"cyphes-wallet coinbase policy v1");

/// Connects to a node's light-wallet gRPC server.
pub async fn connect(server: &str) -> Result<Client> {
    let channel = Channel::from_shared(server.to_string())
        .map_err(|e| err("invalid server URL", e))?
        .connect()
        .await
        .map_err(|e| err(&format!("cannot connect to {server}"), e))?;
    Ok(CompactTxStreamerClient::new(channel))
}

/// The node's chain tip height.
pub async fn tip_height(client: &mut Client) -> Result<u32> {
    let tip = client
        .get_latest_block(ChainSpec {})
        .await
        .map_err(|e| err("get_latest_block", e))?
        .into_inner();
    u32::try_from(tip.height).map_err(|e| err("tip height", e))
}

/// The account birthday for a wallet whose first possible transaction is at
/// `birthday`: the note commitment trees as of the block before it.
async fn birthday_at(
    client: &mut Client,
    network: Network,
    birthday: u32,
) -> Result<AccountBirthday> {
    let prior = birthday.saturating_sub(1);
    if prior == 0 {
        // Every note commitment tree is empty after genesis, which pays
        // nothing. (The gRPC server reads height 0 as "no height given".)
        let mut hash: [u8; 32] = hex::decode(network.params().genesis_hash)
            .map_err(|e| err("genesis hash", e))?
            .try_into()
            .map_err(|_| Error("genesis hash is not 32 bytes".into()))?;
        hash.reverse(); // display order to internal order
        return Ok(AccountBirthday::from_parts(
            ChainState::empty(BlockHeight::from_u32(0), BlockHash(hash)),
            None,
        ));
    }
    let treestate = client
        .get_tree_state(BlockId {
            height: u64::from(prior),
            hash: vec![],
        })
        .await
        .map_err(|e| err("get_tree_state", e))?
        .into_inner();
    AccountBirthday::from_treestate(treestate, None)
        .map_err(|e| err("account birthday", format!("{e:?}")))
}

/// Checks a coinbase confirmation policy for `network`: at least 1
/// everywhere, and on mainnet at least the default
/// [`cyphes_params::WALLET_COINBASE_CONFIRMATIONS`]. Mined value can vanish
/// in a reorganisation, so a mainnet wallet never spends it sooner.
pub fn check_coinbase_confirmations(network: Network, confirmations: u32) -> Result<()> {
    let minimum = match network {
        Network::Mainnet => cyphes_params::WALLET_COINBASE_CONFIRMATIONS,
        Network::Testnet | Network::Regtest => 1,
    };
    if confirmations < minimum {
        return Err(Error(format!(
            "coinbase confirmations must be at least {minimum} on {network:?}"
        )));
    }
    Ok(())
}

/// Parses a CASH amount such as `1000` or `0.00000001` into base units.
pub fn parse_cash(amount: &str) -> Result<Zatoshis> {
    let bad = || Error(format!("invalid CASH amount {amount:?}"));
    let (whole, frac) = amount.split_once('.').unwrap_or((amount, ""));
    if whole.is_empty() && frac.is_empty()
        || frac.len() > 8
        || !frac.chars().all(|c| c.is_ascii_digit())
    {
        return Err(bad());
    }
    let whole: u64 = if whole.is_empty() {
        0
    } else {
        whole.parse().map_err(|_| bad())?
    };
    let frac: u64 = format!("{frac:0<8}").parse().map_err(|_| bad())?;
    let base = whole
        .checked_mul(COIN)
        .and_then(|w| w.checked_add(frac))
        .ok_or_else(bad)?;
    Zatoshis::from_u64(base).map_err(|_| bad())
}

/// Formats base units as CASH with 8 decimals.
pub fn format_cash(base: u64) -> String {
    format!("{}.{:08}", base / COIN, base % COIN)
}

/// Wallet balances, in base units.
#[derive(Clone, Debug, serde::Serialize)]
pub struct Balance {
    /// Everything the wallet holds.
    pub total: u64,
    /// Spendable now under the confirmation policy.
    pub spendable: u64,
    /// Received or change value waiting for confirmations.
    pub pending: u64,
    /// Coinbase value waiting for the coinbase confirmation policy.
    pub coinbase_locked: u64,
    /// The chain tip the wallet knows of.
    pub chain_tip: u32,
    /// The height up to which the wallet has scanned every block.
    pub fully_scanned: u32,
}

/// A transaction built by the wallet.
#[derive(Clone, Debug)]
pub struct Built {
    pub txid: TxId,
    pub raw: Vec<u8>,
    pub fee: u64,
}

/// A wallet transaction, from the wallet's point of view.
#[derive(Clone, Debug, serde::Serialize)]
pub struct HistoryEntry {
    pub txid: String,
    pub mined_height: Option<u32>,
    /// Net change to the wallet's balance, in base units.
    pub delta: i64,
    pub fee: Option<u64>,
    pub is_coinbase: bool,
}

/// A CASH wallet: one ZIP 32 account in a SQLite database.
pub struct Wallet {
    network: Network,
    path: PathBuf,
    db: Db,
    account: AccountUuid,
    coinbase_confirmations: u32,
}

impl Wallet {
    fn open_db(path: &Path, network: Network) -> Result<Db> {
        WalletDb::for_path(path, network, SystemClock, rand::rngs::OsRng)
            .map_err(|e| err("open wallet database", e))
    }

    /// Creates a new wallet database at `path` for `seed`, which receives
    /// nothing before `birthday` (the height of its first possible
    /// transaction).
    pub async fn create(
        path: &Path,
        network: Network,
        seed: &SecretVec<u8>,
        birthday: u32,
        client: &mut Client,
    ) -> Result<Wallet> {
        if path.exists() {
            return Err(Error(format!("{} already exists", path.display())));
        }
        let mut db = Self::open_db(path, network)?;
        init_wallet_db(&mut db, Some(SecretVec::new(seed.expose_secret().clone())))
            .map_err(|e| err("initialise wallet database", e))?;
        let birthday = birthday_at(client, network, birthday).await?;
        let (account, _usk) = db
            .create_account("cash", seed, &birthday, None)
            .map_err(|e| err("create account", e))?;
        let tip = tip_height(client).await?;
        db.update_chain_tip(BlockHeight::from_u32(tip))
            .map_err(|e| err("record chain tip", e))?;
        Ok(Wallet {
            network,
            path: path.to_owned(),
            db,
            account,
            coinbase_confirmations: cyphes_params::WALLET_COINBASE_CONFIRMATIONS,
        })
    }

    /// Opens an existing wallet database.
    pub fn open(path: &Path, network: Network) -> Result<Wallet> {
        if !path.exists() {
            return Err(Error(format!("no wallet at {}", path.display())));
        }
        let db = Self::open_db(path, network)?;
        let account = *db
            .get_account_ids()
            .map_err(|e| err("read accounts", e))?
            .first()
            .ok_or_else(|| Error("wallet has no account".into()))?;
        Ok(Wallet {
            network,
            path: path.to_owned(),
            db,
            account,
            coinbase_confirmations: cyphes_params::WALLET_COINBASE_CONFIRMATIONS,
        })
    }

    pub fn network(&self) -> Network {
        self.network
    }

    /// Sets the confirmations coinbase notes need before they can be spent
    /// (default [`cyphes_params::WALLET_COINBASE_CONFIRMATIONS`]). This is
    /// wallet policy, not consensus. Test networks may lower it; mainnet may
    /// only raise it (see [`check_coinbase_confirmations`]).
    pub fn set_coinbase_confirmations(&mut self, confirmations: u32) -> Result<()> {
        check_coinbase_confirmations(self.network, confirmations)?;
        self.coinbase_confirmations = confirmations;
        self.apply_coinbase_policy().map(|_| ())
    }

    /// The wallet's Ironwood-only unified address: the account's default
    /// address, so the same seed always shows the same address, including
    /// after a restore. (The wallet receives on every diversified address of
    /// the account, not just this one.)
    pub fn address(&mut self) -> Result<String> {
        let account = self
            .db
            .get_account(self.account)
            .map_err(|e| err("read account", e))?
            .ok_or_else(|| Error("wallet account missing".into()))?;
        let (ua, _) = account
            .ufvk()
            .ok_or_else(|| Error("account has no viewing key".into()))?
            .default_address(UnifiedAddressRequest::ORCHARD)
            .map_err(|e| err("derive address", format!("{e:?}")))?;
        Ok(ua.encode(&self.network))
    }

    /// Scans the chain until the wallet is up to date, answers the wallet's
    /// transaction data requests, then applies the coinbase confirmation
    /// policy.
    pub async fn sync(&mut self, client: &mut Client) -> Result<()> {
        let cache = MemoryBlockCache::default();
        sync::run(client, &self.network, &cache, &mut self.db, 1_000)
            .await
            .map_err(|e| err("sync", e))?;
        self.answer_data_requests(client).await?;
        self.apply_coinbase_policy()?;
        Ok(())
    }

    /// Fetches full transactions and their chain status for the wallet.
    ///
    /// Compact blocks carry no expiry heights, fees or memos, and after a
    /// reorganisation the wallet needs to know whether an orphaned transaction
    /// was mined again, waits in the mempool, or is gone; otherwise an
    /// orphaned payment could stay "pending" forever.
    async fn answer_data_requests(&mut self, client: &mut Client) -> Result<()> {
        let requests = self
            .db
            .transaction_data_requests()
            .map_err(|e| err("read transaction data requests", e))?;
        for request in requests {
            let (txid, enhance) = match request {
                TransactionDataRequest::GetStatus(txid) => (txid, false),
                TransactionDataRequest::Enhancement(txid) => (txid, true),
                // CYPHES has no transparent addresses.
                #[allow(unreachable_patterns)]
                _ => continue,
            };
            let response = client
                .get_transaction(TxFilter {
                    block: None,
                    index: 0,
                    hash: txid.as_ref().to_vec(),
                })
                .await;
            let status = match response {
                Ok(raw) => {
                    let raw = raw.into_inner();
                    // 0: in the mempool; u64::MAX: only on a side chain.
                    let mined = (raw.height != 0 && raw.height != u64::MAX)
                        .then(|| u32::try_from(raw.height).map(BlockHeight::from_u32))
                        .transpose()
                        .map_err(|e| err("transaction height", e))?;
                    if enhance {
                        let tip = self.chain_tip()?;
                        let height = mined.unwrap_or(BlockHeight::from_u32(tip + 1));
                        let tx = Transaction::read(
                            &raw.data[..],
                            BranchId::for_height(&self.network, height),
                        )
                        .map_err(|e| err("parse transaction", e))?;
                        decrypt_and_store_transaction(&self.network, &mut self.db, &tx, mined)
                            .map_err(|e| err("store transaction", e))?;
                    }
                    mined.map_or(TransactionStatus::NotInMainChain, TransactionStatus::Mined)
                }
                Err(status) if status.code() == tonic::Code::NotFound => {
                    TransactionStatus::TxidNotRecognized
                }
                Err(e) => return Err(err("get_transaction", e)),
            };
            self.db
                .set_transaction_status(txid, status)
                .map_err(|e| err("record transaction status", e))?;
        }
        Ok(())
    }

    /// Locks every coinbase note until it has the wallet's coinbase
    /// confirmations ([`cyphes_params::WALLET_COINBASE_CONFIRMATIONS`] by
    /// default).
    ///
    /// A coinbase is always the first transaction in its block, so its
    /// notes are the ones received in a transaction with index 0. A note
    /// mined at height `h` is locked through `h + N - 1` and becomes
    /// spendable for a transaction targeting height `h + N`, when it has `N`
    /// confirmations.
    fn apply_coinbase_policy(&mut self) -> Result<usize> {
        let tip = self.chain_tip()?;
        let conn =
            rusqlite::Connection::open(&self.path).map_err(|e| err("open wallet database", e))?;
        let mut stmt = conn
            .prepare(
                "SELECT t.txid, n.action_index, t.mined_height
                 FROM ironwood_received_notes n
                 JOIN transactions t ON t.id_tx = n.transaction_id
                 WHERE t.tx_index = 0 AND t.mined_height IS NOT NULL",
            )
            .map_err(|e| err("query coinbase notes", e))?;
        let notes: Vec<(Vec<u8>, u32, u32)> = stmt
            .query_map([], |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)))
            .map_err(|e| err("query coinbase notes", e))?
            .collect::<std::result::Result<_, _>>()
            .map_err(|e| err("read coinbase notes", e))?;

        let mut locked = 0;
        for (txid, action_index, mined_height) in notes {
            let txid = TxId::from_bytes(
                txid.try_into()
                    .map_err(|_| Error("bad txid in wallet".into()))?,
            );
            let output = OutputRef::new(
                txid,
                PoolType::Shielded(ShieldedPool::Ironwood),
                action_index,
            );
            // Re-derive every lock from the current policy and height: a lock
            // taken under a stricter policy, or before a reorg moved the
            // coinbase, must not outlive it.
            self.db
                .unlock_output(&output, COINBASE_POLICY_OWNER)
                .map_err(|e| err("unlock coinbase note", format!("{e:?}")))?;
            let expiry = mined_height + self.coinbase_confirmations - 1;
            if expiry <= tip {
                continue; // mature
            }
            locked += self
                .db
                .lock_outputs(
                    &[output],
                    COINBASE_POLICY_OWNER,
                    BlockHeight::from_u32(expiry),
                )
                .map_err(|e| err("lock coinbase note", format!("{e:?}")))?;
        }
        Ok(locked)
    }

    fn chain_tip(&self) -> Result<u32> {
        Ok(self
            .db
            .chain_height()
            .map_err(|e| err("read chain tip", e))?
            .map_or(0, u32::from))
    }

    /// Balances under `policy` (ZIP 315 trusted/untrusted confirmations).
    pub fn balance(&self, policy: ConfirmationsPolicy) -> Result<Balance> {
        let summary = self
            .db
            .get_wallet_summary(policy)
            .map_err(|e| err("wallet summary", e))?
            .ok_or_else(|| Error("wallet has not synced yet".into()))?;
        let account = summary
            .account_balances()
            .get(&self.account)
            .ok_or_else(|| Error("no balance for the wallet's account".into()))?;
        let ironwood = account.ironwood_balance();
        Ok(Balance {
            total: ironwood.total().into_u64(),
            spendable: ironwood.spendable_value().into_u64(),
            pending: (ironwood.change_pending_confirmation()
                + ironwood.value_pending_spendability())
            .map_or(u64::MAX, Zatoshis::into_u64),
            coinbase_locked: ironwood.locked_value().into_u64(),
            chain_tip: u32::from(summary.chain_tip_height()),
            fully_scanned: u32::from(summary.fully_scanned_height()),
        })
    }

    /// Builds (and stores, but does not broadcast) a transaction paying
    /// `amount` to `to`, which must be a unified address with an Ironwood
    /// receiver.
    pub fn build_payment(
        &mut self,
        seed: &SecretVec<u8>,
        to: &str,
        amount: Zatoshis,
        memo: Option<&str>,
        policy: ConfirmationsPolicy,
    ) -> Result<Built> {
        let recipient = Address::decode(&self.network, to)
            .ok_or_else(|| Error(format!("not a {} address: {to}", self.network)))?;
        match &recipient {
            Address::Unified(ua) if ua.orchard().is_some() => {}
            _ => {
                return Err(Error(
                    "CASH can only be sent to a unified address with an Ironwood receiver".into(),
                ))
            }
        }
        let memo = memo
            .map(|m| {
                zcash_protocol::memo::MemoBytes::from_bytes(m.as_bytes())
                    .map_err(|e| err("memo", format!("{e:?}")))
            })
            .transpose()?;

        let proposal = propose_standard_transfer_to_address::<_, _, std::convert::Infallible>(
            &mut self.db,
            &self.network,
            StandardFeeRule::Zip317,
            self.account,
            policy,
            &recipient,
            amount,
            memo,
            None,
            ShieldedPool::Ironwood,
            None,
            None,
        )
        .map_err(|e| err("propose payment", e))?;
        let fee = proposal
            .steps()
            .iter()
            .map(|step| step.balance().fee_required().into_u64())
            .sum();

        let usk = UnifiedSpendingKey::from_seed(
            &self.network,
            seed.expose_secret(),
            zip32::AccountId::ZERO,
        )
        .map_err(|e| err("derive spending key", format!("{e:?}")))?;
        let txids = create_proposed_transactions::<
            _,
            _,
            std::convert::Infallible,
            _,
            std::convert::Infallible,
            _,
        >(
            &mut self.db,
            &self.network,
            &NoSapling,
            &NoSapling,
            &SpendingKeys::from_unified_spending_key(usk),
            OvkPolicy::Sender,
            &proposal,
            None,
        )
        .map_err(|e| err("create transaction", e))?;
        let txid = *txids.first();
        let raw = self.raw_transaction(txid)?;
        Ok(Built { txid, raw, fee })
    }

    /// The serialized transaction `txid`, if the wallet has it.
    pub fn raw_transaction(&self, txid: TxId) -> Result<Vec<u8>> {
        let tx = self
            .db
            .get_transaction(txid)
            .map_err(|e| err("read transaction", e))?
            .ok_or_else(|| Error(format!("transaction {txid} not in wallet")))?;
        let mut raw = Vec::new();
        tx.write(&mut raw)
            .map_err(|e| err("serialize transaction", e))?;
        Ok(raw)
    }

    /// The wallet's transactions, oldest first.
    pub fn history(&self) -> Result<Vec<HistoryEntry>> {
        let conn =
            rusqlite::Connection::open(&self.path).map_err(|e| err("open wallet database", e))?;
        let mut stmt = conn
            .prepare(
                "SELECT v.txid, v.mined_height, v.account_balance_delta, v.fee_paid, t.tx_index
                 FROM v_transactions v
                 LEFT JOIN transactions t ON t.txid = v.txid
                 ORDER BY v.mined_height IS NULL, v.mined_height, v.txid",
            )
            .map_err(|e| err("query history", e))?;
        let rows = stmt
            .query_map([], |row| {
                let txid: Vec<u8> = row.get(0)?;
                let tx_index: Option<u32> = row.get(4)?;
                Ok(HistoryEntry {
                    txid: TxId::from_bytes(txid.try_into().unwrap_or([0; 32])).to_string(),
                    mined_height: row.get(1)?,
                    delta: row.get(2)?,
                    fee: row.get(3)?,
                    is_coinbase: tx_index == Some(0),
                })
            })
            .map_err(|e| err("query history", e))?;
        rows.collect::<std::result::Result<_, _>>()
            .map_err(|e| err("read history", e))
    }
}

/// Broadcasts a serialized transaction through the node.
pub async fn broadcast(client: &mut Client, raw: &[u8]) -> Result<()> {
    let response = client
        .send_transaction(RawTransaction {
            data: raw.to_vec(),
            height: 0,
        })
        .await
        .map_err(|e| err("send_transaction", e))?
        .into_inner();
    if response.error_code == 0 {
        Ok(())
    } else {
        Err(Error(format!(
            "node rejected transaction ({}): {}",
            response.error_code, response.error_message
        )))
    }
}

/// A confirmation policy: `trusted` confirmations for the wallet's own
/// change, `untrusted` for everything received.
pub fn policy(trusted: u32, untrusted: u32) -> Result<ConfirmationsPolicy> {
    let nz =
        |n: u32| NonZeroU32::new(n).ok_or_else(|| Error("confirmations must be at least 1".into()));
    ConfirmationsPolicy::new(nz(trusted)?, nz(untrusted)?)
        .map_err(|e| err("confirmation policy", e))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cash_amounts_parse_exactly() {
        assert_eq!(parse_cash("1000").unwrap().into_u64(), 1_000 * COIN);
        assert_eq!(parse_cash("0.00000001").unwrap().into_u64(), 1);
        assert_eq!(
            parse_cash("25000000.5").unwrap().into_u64(),
            25_000_000 * COIN + COIN / 2
        );
        assert_eq!(format_cash(25_000_000 * COIN + 1), "25000000.00000001");
        for bad in ["", ".", "1.000000001", "-1", "1e3", "10000000001"] {
            assert!(parse_cash(bad).is_err(), "{bad}");
        }
    }

    #[test]
    fn mainnet_coinbase_policy_can_only_be_raised() {
        let default = cyphes_params::WALLET_COINBASE_CONFIRMATIONS;
        for n in [0, 1, default - 1] {
            assert!(
                check_coinbase_confirmations(Network::Mainnet, n).is_err(),
                "{n}"
            );
        }
        for n in [default, default + 1] {
            assert!(
                check_coinbase_confirmations(Network::Mainnet, n).is_ok(),
                "{n}"
            );
        }
        for network in [Network::Testnet, Network::Regtest] {
            assert!(check_coinbase_confirmations(network, 0).is_err());
            assert!(check_coinbase_confirmations(network, 1).is_ok());
        }
    }

    #[test]
    fn the_cyphes_patch_is_in_effect() {
        // If Cargo ever resolves the registry's zcash_protocol instead of the
        // fork, these are Zcash's values again.
        assert_eq!(zcash_protocol::value::MAX_MONEY, cyphes_params::MAX_MONEY);
        assert_eq!(
            u32::from(zcash_protocol::consensus::BranchId::Nu6_3),
            cyphes_params::network::CONSENSUS_BRANCH_ID_V1
        );
        assert_eq!(
            zcash_protocol::constants::mainnet::HRP_UNIFIED_ADDRESS,
            "cyph"
        );
        assert!(Zatoshis::from_u64(50_000_000 * COIN).is_ok());
    }
}
