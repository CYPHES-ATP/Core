//! Definitions of Zebra chain constants, including:
//! - slow start interval,
//! - slow start shift,
//! - maximum reorg height

use crate::block::Height;

/// Zcash's slow-start mining period. CYPHES pays the full subsidy from block 1,
/// so this is zero.
pub const SLOW_START_INTERVAL: Height = Height(0);

/// `SlowStartShift()` as described in [protocol specification §7.8][7.8]
///
/// [7.8]: https://zips.z.cash/protocol/protocol.pdf#subsidies
///
/// This calculation is exact, because `SLOW_START_INTERVAL` is divisible by 2.
pub const SLOW_START_SHIFT: Height = Height(SLOW_START_INTERVAL.0 / 2);

/// The maximum chain reorganisation height.
///
/// This threshold determines the maximum length of the best non-finalized
/// chain. Once the chain grows past this height, Zebra finalizes its oldest
/// blocks; deeper reorganisations are outside Zebra's rollback window.
///
/// This is a local-only node policy; it is not part of consensus. The window is
/// sized as a defence-in-depth measure against sustained consensus splits.
//
// TODO: change to HeightDiff
pub const MAX_BLOCK_REORG_HEIGHT: u32 = 1000;

/// Magic numbers used to identify different CYPHES networks.
///
/// Defined in `cyphes_params::network`: the first four bytes of
/// `SHA-256("CYPHES <network>")`.
pub mod magics {
    use crate::parameters::network::magic::Magic;

    /// The production mainnet.
    pub const MAINNET: Magic = Magic(cyphes_params::network::MAINNET.magic);
    /// The testnet.
    pub const TESTNET: Magic = Magic(cyphes_params::network::TESTNET.magic);
    /// The regtest.
    pub const REGTEST: Magic = Magic(cyphes_params::network::REGTEST.magic);
}

/// The block heights at which network upgrades activate.
///
/// CYPHES has no upgrade history: every Zcash upgrade through NU6.3 (Ironwood)
/// is in force from block 1, so every constant here is `Height(1)`.
pub mod activation_heights {
    /// Network upgrade activation heights for Testnet.
    pub mod testnet {
        use crate::block::Height;

        /// The block height at which `BeforeOverwinter` activates on Testnet.
        pub const BEFORE_OVERWINTER: Height = Height(1);
        /// The block height at which `Overwinter` activates on Testnet.
        pub const OVERWINTER: Height = Height(1);
        /// The block height at which `Sapling` activates on Testnet.
        pub const SAPLING: Height = Height(1);
        /// The block height at which `Blossom` activates on Testnet.
        pub const BLOSSOM: Height = Height(1);
        /// The block height at which `Heartwood` activates on Testnet.
        pub const HEARTWOOD: Height = Height(1);
        /// The block height at which `Canopy` activates on Testnet.
        pub const CANOPY: Height = Height(1);
        /// The block height at which `NU5` activates on Testnet.
        pub const NU5: Height = Height(1);
        /// The block height at which `NU6` activates on Testnet.
        pub const NU6: Height = Height(1);
        /// The block height at which `NU6.1` activates on Testnet.
        pub const NU6_1: Height = Height(1);
        /// The block height at which `NU6.2` activates on Testnet.
        pub const NU6_2: Height = Height(1);
        /// The block height at which `NU6.3` activates on Testnet.
        pub const NU6_3: Height = Height(1);
    }

    /// Network upgrade activation heights for Mainnet.
    pub mod mainnet {
        use crate::block::Height;

        /// The block height at which `BeforeOverwinter` activates on Mainnet.
        pub const BEFORE_OVERWINTER: Height = Height(1);
        /// The block height at which `Overwinter` activates on Mainnet.
        pub const OVERWINTER: Height = Height(1);
        /// The block height at which `Sapling` activates on Mainnet.
        pub const SAPLING: Height = Height(1);
        /// The block height at which `Blossom` activates on Mainnet.
        pub const BLOSSOM: Height = Height(1);
        /// The block height at which `Heartwood` activates on Mainnet.
        pub const HEARTWOOD: Height = Height(1);
        /// The block height at which `Canopy` activates on Mainnet.
        pub const CANOPY: Height = Height(1);
        /// The block height at which `NU5` activates on Mainnet.
        pub const NU5: Height = Height(1);
        /// The block height at which `NU6` activates on Mainnet.
        pub const NU6: Height = Height(1);
        /// The block height at which `NU6.1` activates on Mainnet.
        pub const NU6_1: Height = Height(1);
        /// The block height at which `NU6.2` activates on Mainnet.
        pub const NU6_2: Height = Height(1);
        /// The block height at which `NU6.3` activates on Mainnet.
        pub const NU6_3: Height = Height(1);
    }
}
