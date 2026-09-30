use std::{
    collections::{BTreeMap, HashMap},
    fs::DirEntry,
    io::{self, ErrorKind, Write},
    path::{Path, PathBuf},
    sync::Arc,
    time::Duration,
};

use hex::ToHex;
use zebra_chain::{
    amount::Amount,
    block::{self, Block, Height},
    serialization::{ZcashDeserializeInto, ZcashSerialize},
};

use crate::{
    ContextuallyVerifiedBlock, IntoDisk, NonFinalizedState, SemanticallyVerifiedBlock,
    WatchReceiver, ZebraDb,
};

#[cfg(not(test))]
use crate::service::write::validate_and_commit_non_finalized;

/// The minimum duration that Zebra will wait between updates to the non-finalized state backup cache.
pub(crate) const MIN_DURATION_BETWEEN_BACKUP_UPDATES: Duration = Duration::from_secs(5);

/// Accepts an optional path to the non-finalized state backup directory and a handle to the database.
///
/// Looks for blocks above the finalized tip height in the backup directory (if a path was provided) and
/// attempts to commit them to the non-finalized state.
///
/// Returns the resulting non-finalized state.
pub(super) fn restore_backup(
    mut non_finalized_state: NonFinalizedState,
    backup_dir_path: &Path,
    finalized_state: &ZebraDb,
) -> NonFinalizedState {
    let mut store: BTreeMap<Height, Vec<SemanticallyVerifiedBlock>> = BTreeMap::new();

    for block in read_non_finalized_blocks_from_backup(backup_dir_path, finalized_state) {
        store.entry(block.height).or_default().push(block);
    }

    for (height, blocks) in store {
        for block in blocks {
            #[cfg(test)]
            let commit_result = if non_finalized_state
                .any_chain_contains(&block.block.header.previous_block_hash)
            {
                non_finalized_state.commit_block(block, finalized_state)
            } else {
                non_finalized_state.commit_new_chain(block, finalized_state)
            };

            #[cfg(not(test))]
            let commit_result =
                validate_and_commit_non_finalized(finalized_state, &mut non_finalized_state, block);

            // Re-computes the block hash in case the hash from the filename is wrong.
            if let Err(commit_error) = commit_result {
                tracing::warn!(
                    ?commit_error,
                    ?height,
                    "failed to commit non-finalized block from backup directory"
                );
            }
        }
    }

    non_finalized_state
}

/// Updates the non-finalized state backup cache by writing any blocks that are in the
/// non-finalized state but missing in the backup cache, and deleting any backup files
/// that are no longer present in the non-finalized state.
///
/// `backup_blocks` should be the current contents of the backup directory, obtained by
/// calling [`list_backup_dir_entries`] before the non-finalized state was updated.
///
/// This function performs blocking I/O and should be called from a blocking context,
/// or wrapped in [`tokio::task::spawn_blocking`].
pub(super) fn update_non_finalized_state_backup(
    backup_dir_path: &Path,
    non_finalized_state: &NonFinalizedState,
    mut backup_blocks: HashMap<block::Hash, PathBuf>,
) {
    for block in non_finalized_state
        .chain_iter()
        .flat_map(|chain| chain.blocks.values())
        // Remove blocks from `backup_blocks` that are present in the non-finalized state
        .filter(|block| backup_blocks.remove(&block.hash).is_none())
    {
        // This loop will typically iterate only once, but may write multiple blocks if it misses
        // some non-finalized state changes while waiting for I/O ops.
        write_backup_block(backup_dir_path, block);
    }

    // Remove any backup blocks that are not present in the non-finalized state
    for (_, outdated_backup_block_path) in backup_blocks {
        if let Err(delete_error) = std::fs::remove_file(outdated_backup_block_path) {
            tracing::warn!(?delete_error, "failed to delete backup block file");
        }
    }
}

/// Updates the non-finalized state backup cache whenever the non-finalized state changes,
/// deleting any outdated backup files and writing any blocks that are in the non-finalized
/// state but missing in the backup cache.
pub(super) async fn run_backup_task(
    mut non_finalized_state_receiver: WatchReceiver<NonFinalizedState>,
    backup_dir_path: PathBuf,
) {
    let err = loop {
        let rate_limit = tokio::time::sleep(MIN_DURATION_BETWEEN_BACKUP_UPDATES);
        let backup_blocks: HashMap<block::Hash, PathBuf> = {
            let backup_dir_path = backup_dir_path.clone();
            tokio::task::spawn_blocking(move || list_backup_dir_entries(&backup_dir_path))
                .await
                .expect("failed to join blocking task when reading in backup task")
                .collect()
        };

        if let (Err(err), _) = tokio::join!(non_finalized_state_receiver.changed(), rate_limit) {
            break err;
        };

        let latest_non_finalized_state = non_finalized_state_receiver.cloned_watch_data();

        let backup_dir_path = backup_dir_path.clone();
        tokio::task::spawn_blocking(move || {
            update_non_finalized_state_backup(
                &backup_dir_path,
                &latest_non_finalized_state,
                backup_blocks,
            );
        })
        .await
        .expect("failed to join blocking task when writing in backup task");
    };

    tracing::warn!(
        ?err,
        "got recv error waiting on non-finalized state change, is Zebra shutting down?"
    )
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct NonFinalizedBlockBackup {
    block: Arc<Block>,
    deferred_pool_balance_change: Amount,
}

impl From<&ContextuallyVerifiedBlock> for NonFinalizedBlockBackup {
    fn from(cv_block: &ContextuallyVerifiedBlock) -> Self {
        Self {
            block: cv_block.block.clone(),
            deferred_pool_balance_change: cv_block.chain_value_pool_change.deferred_amount(),
        }
    }
}

impl NonFinalizedBlockBackup {
    /// Encodes a [`NonFinalizedBlockBackup`] as a vector of bytes.
    fn as_bytes(&self) -> Vec<u8> {
        let block_bytes = self
            .block
            .zcash_serialize_to_vec()
            .expect("verified block header version should be valid");

        let deferred_pool_balance_change_bytes =
            self.deferred_pool_balance_change.as_bytes().to_vec();

        [deferred_pool_balance_change_bytes, block_bytes].concat()
    }

    /// Constructs a new [`NonFinalizedBlockBackup`] from a vector of bytes.
    #[allow(clippy::unwrap_in_result)]
    fn from_bytes(bytes: Vec<u8>) -> Result<Self, io::Error> {
        let (deferred_pool_balance_change_bytes, block_bytes) = bytes
            .split_at_checked(size_of::<Amount>())
            .ok_or(io::Error::new(
                ErrorKind::InvalidInput,
                "input is too short",
            ))?;

        Ok(Self {
            block: Arc::new(
                block_bytes
                    .zcash_deserialize_into()
                    .map_err(|err| io::Error::new(ErrorKind::InvalidData, err))?,
            ),
            deferred_pool_balance_change: Amount::from_bytes(
                deferred_pool_balance_change_bytes
                    .try_into()
                    .expect("slice from `split_at_checked()` should fit in [u8; 8]"),
            )
            .map_err(|err| io::Error::new(ErrorKind::InvalidData, err))?,
        })
    }
}

/// Writes a block to a file in the provided non-finalized state backup cache directory path.
fn write_backup_block(backup_dir_path: &Path, block: &ContextuallyVerifiedBlock) {
    let backup_block_file_name: String = block.hash.encode_hex();
    let non_finalized_block_backup: NonFinalizedBlockBackup = block.into();

    if let Err(err) = write_file_durably(
        backup_dir_path,
        &backup_block_file_name,
        &non_finalized_block_backup.as_bytes(),
    ) {
        tracing::warn!(?err, "failed to write non-finalized state backup block");
    }
}

/// CYPHES: writes `bytes` to `dir/name` so that a crash or power loss leaves
/// either the complete file or none at all.
///
/// The bytes go to `dir/name.tmp`, are flushed to disk, and the file is then
/// renamed into place. A `.tmp` file left by a crash is not named after a
/// block hash, so the next directory listing deletes it (see
/// [`process_backup_dir_entry`]).
fn write_file_durably(dir: &Path, name: &str, bytes: &[u8]) -> io::Result<()> {
    let path = dir.join(name);
    let temp_path = dir.join(format!("{name}.tmp"));

    let result = (|| {
        let mut file = std::fs::File::create(&temp_path)?;
        file.write_all(bytes)?;
        file.sync_all()?;
        drop(file);
        std::fs::rename(&temp_path, &path)
    })();
    if result.is_err() {
        let _ = std::fs::remove_file(&temp_path);
    }
    result?;

    // Make the rename durable too. Not every platform can sync a directory;
    // the file itself is already complete on disk.
    #[cfg(unix)]
    if let Err(err) = std::fs::File::open(dir).and_then(|dir| dir.sync_all()) {
        tracing::debug!(?err, "failed to sync non-finalized state backup directory");
    }

    Ok(())
}

/// Reads blocks from the provided non-finalized state backup directory path.
///
/// Returns any blocks that are valid and not present in the finalized state.
fn read_non_finalized_blocks_from_backup<'a>(
    backup_dir_path: &Path,
    finalized_state: &'a ZebraDb,
) -> impl Iterator<Item = SemanticallyVerifiedBlock> + 'a {
    list_backup_dir_entries(backup_dir_path)
        // It's okay to leave the file here, the backup task will delete it as long as
        // the block is not added to the non-finalized state.
        .filter(|&(block_hash, _)| !finalized_state.contains_hash(block_hash))
        .filter_map(|(block_hash, file_path)| match std::fs::read(file_path) {
            Ok(block_bytes) => Some((block_hash, block_bytes)),
            Err(err) => {
                tracing::warn!(?err, "failed to open non-finalized state backup block file");
                None
            }
        })
        .filter_map(|(expected_block_hash, backup_block_file_contents)| {
            match NonFinalizedBlockBackup::from_bytes(backup_block_file_contents) {
                Ok(NonFinalizedBlockBackup {
                    block,
                    deferred_pool_balance_change: _,
                }) if block.coinbase_height().is_some() => {
                    let block = SemanticallyVerifiedBlock::from(block);
                    if block.hash != expected_block_hash {
                        tracing::warn!(
                            block_hash = ?block.hash,
                            ?expected_block_hash,
                            "wrong block hash in file name"
                        );
                    }
                    Some(block)
                }
                Ok(block) => {
                    tracing::warn!(
                        ?block,
                        "invalid non-finalized backup block, missing coinbase height"
                    );
                    None
                }
                Err(err) => {
                    tracing::warn!(
                        ?err,
                        "failed to deserialize non-finalized backup data into block"
                    );
                    None
                }
            }
        })
}

/// Accepts a backup directory path, opens the directory, converts its entries
/// filenames to block hashes, and deletes any entries with invalid file names.
///
/// # Panics
///
/// If the provided path cannot be opened as a directory.
/// See [`read_backup_dir`] for more details.
pub(super) fn list_backup_dir_entries(
    backup_dir_path: &Path,
) -> impl Iterator<Item = (block::Hash, PathBuf)> {
    read_backup_dir(backup_dir_path).filter_map(process_backup_dir_entry)
}

/// Accepts a backup directory path and opens the directory.
///
/// Returns an iterator over all [`DirEntry`]s in the directory that are successfully read.
///
/// # Panics
///
/// If the provided path cannot be opened as a directory.
fn read_backup_dir(backup_dir_path: &Path) -> impl Iterator<Item = DirEntry> {
    std::fs::read_dir(backup_dir_path)
        .expect("failed to read non-finalized state backup directory")
        .filter_map(|entry| match entry {
            Ok(entry) => Some(entry),
            Err(io_err) => {
                tracing::warn!(
                    ?io_err,
                    "failed to read DirEntry in non-finalized state backup dir"
                );

                None
            }
        })
}

/// Accepts a [`DirEntry`] from the non-finalized state backup directory and
/// parses the filename into a block hash.
///
/// Returns the block hash and the file path if successful, or
/// returns None and deletes the file at the entry path otherwise.
fn process_backup_dir_entry(entry: DirEntry) -> Option<(block::Hash, PathBuf)> {
    let delete_file = || {
        if let Err(delete_error) = std::fs::remove_file(entry.path()) {
            tracing::warn!(?delete_error, "failed to delete backup block file");
        }
    };

    let block_file_name = match entry.file_name().into_string() {
        Ok(block_hash) => block_hash,
        Err(err) => {
            tracing::warn!(
                ?err,
                "failed to convert OsString to String, attempting to delete file"
            );

            delete_file();
            return None;
        }
    };

    let block_hash: block::Hash = match block_file_name.parse() {
        Ok(block_hash) => block_hash,
        Err(err) => {
            tracing::warn!(
                ?err,
                "failed to parse hex-encoded block hash from file name, attempting to delete file"
            );

            delete_file();
            return None;
        }
    };

    Some((block_hash, entry.path()))
}

#[cfg(test)]
mod cyphes_tests {
    use zebra_chain::{block::genesis::genesis_block, parameters::Network};

    use super::*;
    use crate::{service::finalized_state::FinalizedState, Config};

    fn backup_bytes(block: Arc<Block>) -> Vec<u8> {
        NonFinalizedBlockBackup {
            block,
            deferred_pool_balance_change: Amount::zero(),
        }
        .as_bytes()
    }

    #[test]
    fn backup_files_are_replaced_whole_and_leave_no_temporary_file() {
        let dir = tempfile::tempdir().expect("temporary directory");
        write_file_durably(dir.path(), "a", b"first").expect("write");
        write_file_durably(dir.path(), "a", b"second").expect("overwrite");

        assert_eq!(
            std::fs::read(dir.path().join("a")).expect("read"),
            b"second"
        );
        let names: Vec<_> = std::fs::read_dir(dir.path())
            .expect("list")
            .map(|entry| entry.expect("entry").file_name())
            .collect();
        assert_eq!(names, ["a"]);
    }

    #[test]
    fn restore_skips_torn_backup_files_and_deletes_temporary_files() {
        let network = Network::new_regtest(Default::default());
        let finalized_state = FinalizedState::new(
            &Config::ephemeral(),
            &network,
            #[cfg(feature = "elasticsearch")]
            false,
        )
        .expect("ephemeral database");
        let dir = tempfile::tempdir().expect("temporary directory");

        // A complete backup file.
        let whole = genesis_block(&Network::Mainnet);
        let whole_name: String = whole.hash().encode_hex();
        write_file_durably(dir.path(), &whole_name, &backup_bytes(whole.clone())).expect("write");

        // A torn file, as a crash during a plain (non-atomic) write leaves it.
        let torn = genesis_block(&Network::new_default_testnet());
        let torn_bytes = backup_bytes(torn.clone());
        let torn_name: String = torn.hash().encode_hex();
        std::fs::write(
            dir.path().join(torn_name),
            &torn_bytes[..torn_bytes.len() / 2],
        )
        .expect("write torn file");

        // A temporary file left by a crash during an atomic write.
        let temp = dir.path().join(format!("{whole_name}.tmp"));
        std::fs::write(&temp, &torn_bytes).expect("write temporary file");

        let restored: Vec<_> =
            read_non_finalized_blocks_from_backup(dir.path(), &finalized_state.db)
                .map(|block| block.hash)
                .collect();
        assert_eq!(
            restored,
            vec![whole.hash()],
            "only the complete block is restored"
        );
        assert!(!temp.exists(), "the temporary file is deleted");
    }
}
