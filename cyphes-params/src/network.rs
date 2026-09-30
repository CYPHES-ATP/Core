//! Network identity: magic bytes, ports, address prefixes, branch ID.
//!
//! These values are what keep CYPHES nodes and wallets from ever talking to,
//! or signing for, Zcash. Magic bytes are the first four bytes of
//! `SHA-256("CYPHES <network>")`, so they can be re-derived by anyone.

/// Which CYPHES network.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum NetworkKind {
    Mainnet,
    Testnet,
    /// Local development: PoW may be disabled, parameters are not stable.
    Regtest,
}

/// Parameters that identify a CYPHES network.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct NetworkParams {
    pub kind: NetworkKind,
    pub name: &'static str,
    /// P2P message start bytes.
    pub magic: [u8; 4],
    pub default_p2p_port: u16,
    pub default_rpc_port: u16,
    /// Light-wallet gRPC (lightwalletd `CompactTxStreamer`) served by the node.
    pub default_wallet_grpc_port: u16,
    /// Unified address human-readable part (ZIP 316 encoding, Ironwood receiver only).
    pub address_hrp: &'static str,
    /// Unified full viewing key HRP.
    pub ufvk_hrp: &'static str,
    /// Unified incoming viewing key HRP.
    pub uivk_hrp: &'static str,
    /// ZIP 32 coin type for key derivation.
    pub coin_type: u32,
    /// Easiest allowed target, compact encoded.
    pub pow_limit_compact: u32,
    /// Hash of the embedded genesis block, in display (RPC) byte order.
    pub genesis_hash: &'static str,
}

/// Consensus branch ID for the CYPHES v1 rules, used in ZIP 244 signature
/// hashes. Distinct from every Zcash branch ID, so no signature is valid on
/// both chains. First four bytes of `SHA-256("CYPHES consensus branch v1")`.
pub const CONSENSUS_BRANCH_ID_V1: u32 = 0x4535_c5e0;

/// Provisional mainnet ZIP 32 coin type ("CASH" in ASCII).
///
/// Must be registered in SLIP-0044 before mainnet genesis.
pub const PROVISIONAL_MAINNET_COIN_TYPE: u32 = 0x4341_5348;

pub const MAINNET: NetworkParams = NetworkParams {
    kind: NetworkKind::Mainnet,
    name: "mainnet",
    magic: [0x05, 0x05, 0x29, 0xbb],
    default_p2p_port: 2974,
    default_rpc_port: 2975,
    default_wallet_grpc_port: 2976,
    address_hrp: "cyph",
    ufvk_hrp: "cyphview",
    uivk_hrp: "cyphivk",
    coin_type: PROVISIONAL_MAINNET_COIN_TYPE,
    // 0xffff << 232: one valid BeamHash III solution in 256 meets it, so a
    // single GPU can still extend the chain if hashrate collapses.
    pow_limit_compact: 0x2000_ffff,
    // PROVISIONAL: re-mined at launch.
    genesis_hash: "1e0a6479a90c0e735192f51156aabd7593e66dd15e0597677687bcf5ec741da9",
};

pub const TESTNET: NetworkParams = NetworkParams {
    kind: NetworkKind::Testnet,
    name: "testnet",
    magic: [0x76, 0xca, 0x23, 0x8e],
    default_p2p_port: 12974,
    default_rpc_port: 12975,
    default_wallet_grpc_port: 12976,
    address_hrp: "cyphtest",
    ufvk_hrp: "cyphviewtest",
    uivk_hrp: "cyphivktest",
    coin_type: 1,
    pow_limit_compact: 0x2000_ffff,
    genesis_hash: "657e5d5139986a8f5b0aadbd6bc509d0db6b5cea1a332dc7195506fa9e9a029b",
};

pub const REGTEST: NetworkParams = NetworkParams {
    kind: NetworkKind::Regtest,
    name: "regtest",
    magic: [0xd4, 0x4a, 0x72, 0x44],
    default_p2p_port: 22974,
    default_rpc_port: 22975,
    default_wallet_grpc_port: 22976,
    address_hrp: "cyphregtest",
    ufvk_hrp: "cyphviewregtest",
    uivk_hrp: "cyphivkregtest",
    coin_type: 1,
    // 0x7fffff << 232: half of all valid solutions.
    pow_limit_compact: 0x207f_ffff,
    genesis_hash: "5627d4e9cc60cfca84fc5e15ab22b6a7332c8be2b210ef25c158e65b62421b53",
};

impl NetworkKind {
    pub const fn params(self) -> &'static NetworkParams {
        match self {
            NetworkKind::Mainnet => &MAINNET,
            NetworkKind::Testnet => &TESTNET,
            NetworkKind::Regtest => &REGTEST,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use sha2::{Digest, Sha256};

    fn prefix(s: &str) -> [u8; 4] {
        Sha256::digest(s.as_bytes())[..4].try_into().unwrap()
    }

    #[test]
    fn magic_and_branch_id_are_rederivable() {
        for p in [MAINNET, TESTNET, REGTEST] {
            assert_eq!(p.magic, prefix(&format!("CYPHES {}", p.name)), "{}", p.name);
        }
        assert_eq!(
            CONSENSUS_BRANCH_ID_V1.to_be_bytes(),
            prefix("CYPHES consensus branch v1")
        );
    }

    #[test]
    fn nothing_collides_with_zcash() {
        // Zcash mainnet, testnet and regtest magic.
        let zcash_magic = [
            [0x24, 0xe9, 0x27, 0x64],
            [0xfa, 0x1a, 0xf9, 0xbf],
            [0xaa, 0xe8, 0x3f, 0x5f],
        ];
        // Every Zcash consensus branch ID through NU7 and ZFuture.
        let zcash_branch_ids = [
            0x5ba8_1b19u32,
            0x76b8_09bb,
            0x2bb4_0e60,
            0xf5b9_230b,
            0xe9ff_75a6,
            0xc2d6_d0b4,
            0xc8e7_1055,
            0x4dec_4df0,
            0x5437_f330,
            0x37a5_165b,
            0x7719_0ad8,
            0xffff_fffe,
            0xffff_fffd,
        ];
        for p in [MAINNET, TESTNET, REGTEST] {
            assert!(!zcash_magic.contains(&p.magic));
            assert!(!["u", "utest", "uregtest", "zs", "ztestsapling", "t", "tm"]
                .contains(&p.address_hrp));
            assert_ne!(p.coin_type, 133, "133 is Zcash's coin type");
        }
        assert!(!zcash_branch_ids.contains(&CONSENSUS_BRANCH_ID_V1));
    }

    #[test]
    fn networks_are_distinct() {
        let all = [MAINNET, TESTNET, REGTEST];
        for (i, a) in all.iter().enumerate() {
            for b in &all[i + 1..] {
                assert_ne!(a.magic, b.magic);
                assert_ne!(a.address_hrp, b.address_hrp);
                assert_ne!(a.default_p2p_port, b.default_p2p_port);
            }
        }
    }

    #[test]
    fn coin_type_is_a_valid_hardened_index() {
        for p in [MAINNET, TESTNET, REGTEST] {
            assert!(p.coin_type < 1 << 31);
        }
    }
}
