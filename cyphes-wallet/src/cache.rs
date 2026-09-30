//! An in-memory compact block cache for sync.
//!
//! Sync downloads compact blocks, scans them and deletes them; nothing needs
//! to survive a restart, so memory is enough.

use std::{
    collections::BTreeMap,
    convert::Infallible,
    sync::{Arc, Mutex},
};

use async_trait::async_trait;
use zcash_client_backend::{
    data_api::{
        chain::{error::Error as ChainError, BlockCache, BlockSource},
        scanning::ScanRange,
    },
    proto::compact_formats::CompactBlock,
};
use zcash_protocol::consensus::BlockHeight;

/// Compact blocks keyed by height.
#[derive(Clone, Default)]
pub struct MemoryBlockCache {
    blocks: Arc<Mutex<BTreeMap<u32, CompactBlock>>>,
}

fn height_of(block: &CompactBlock) -> u32 {
    u32::try_from(block.height).expect("compact block heights fit in u32")
}

impl BlockSource for MemoryBlockCache {
    type Error = Infallible;

    fn with_blocks<F, WalletErrT>(
        &self,
        from_height: Option<BlockHeight>,
        limit: Option<usize>,
        mut with_block: F,
    ) -> Result<(), ChainError<WalletErrT, Self::Error>>
    where
        F: FnMut(CompactBlock) -> Result<(), ChainError<WalletErrT, Self::Error>>,
    {
        let from = from_height.map_or(0, u32::from);
        let blocks: Vec<CompactBlock> = self
            .blocks
            .lock()
            .expect("cache lock")
            .range(from..)
            .take(limit.unwrap_or(usize::MAX))
            .map(|(_, block)| block.clone())
            .collect();
        blocks.into_iter().try_for_each(&mut with_block)
    }
}

#[async_trait]
impl BlockCache for MemoryBlockCache {
    fn get_tip_height(
        &self,
        range: Option<&ScanRange>,
    ) -> Result<Option<BlockHeight>, Self::Error> {
        let blocks = self.blocks.lock().expect("cache lock");
        let tip = match range {
            Some(range) => blocks
                .keys()
                .rev()
                .find(|h| range.block_range().contains(&BlockHeight::from_u32(**h))),
            None => blocks.keys().next_back(),
        };
        Ok(tip.map(|h| BlockHeight::from_u32(*h)))
    }

    async fn read(&self, range: &ScanRange) -> Result<Vec<CompactBlock>, Self::Error> {
        let start = u32::from(range.block_range().start);
        let end = u32::from(range.block_range().end);
        Ok(self
            .blocks
            .lock()
            .expect("cache lock")
            .range(start..end)
            .map(|(_, block)| block.clone())
            .collect())
    }

    async fn insert(&self, compact_blocks: Vec<CompactBlock>) -> Result<(), Self::Error> {
        let mut blocks = self.blocks.lock().expect("cache lock");
        for block in compact_blocks {
            blocks.insert(height_of(&block), block);
        }
        Ok(())
    }

    async fn delete(&self, range: ScanRange) -> Result<(), Self::Error> {
        let start = u32::from(range.block_range().start);
        let end = u32::from(range.block_range().end);
        self.blocks
            .lock()
            .expect("cache lock")
            .retain(|h, _| !(start..end).contains(h));
        Ok(())
    }
}
