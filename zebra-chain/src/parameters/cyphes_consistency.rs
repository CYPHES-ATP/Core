//! Checks that the librustzcash fork and `cyphes-params` agree.
//!
//! `zcash_protocol` (vendored in `librustzcash/`) hard-codes the CYPHES
//! network constants because it cannot depend on `cyphes-params`. These tests
//! fail if the two ever drift.

use cyphes_params::network::{CONSENSUS_BRANCH_ID_V1, MAINNET, REGTEST, TESTNET};
use zcash_protocol::{
    consensus::{
        BranchId, NetworkType, NetworkUpgrade as ZpNetworkUpgrade, Parameters, MAIN_NETWORK,
        TEST_NETWORK,
    },
    constants::{mainnet, regtest, testnet},
    value::MAX_MONEY,
};

use crate::parameters::{Network, NetworkUpgrade};

#[test]
fn money_matches() {
    assert_eq!(MAX_MONEY, cyphes_params::MAX_MONEY);
    assert_eq!(crate::amount::MAX_MONEY as u64, cyphes_params::MAX_MONEY);
}

#[test]
fn branch_id_matches() {
    assert_eq!(u32::from(BranchId::Nu6_3), CONSENSUS_BRANCH_ID_V1);
    for network in [Network::Mainnet, Network::new_default_testnet()] {
        assert_eq!(
            NetworkUpgrade::Nu6_3.branch_id().map(u32::from),
            Some(CONSENSUS_BRANCH_ID_V1),
        );
        assert_eq!(
            NetworkUpgrade::current(&network, crate::block::Height(1)),
            NetworkUpgrade::Nu6_3,
            "{network}"
        );
    }
    assert_eq!(
        BranchId::for_height(&MAIN_NETWORK, 1.into()),
        BranchId::Nu6_3
    );
    assert_eq!(
        BranchId::for_height(&TEST_NETWORK, 1.into()),
        BranchId::Nu6_3
    );
    assert_eq!(
        MAIN_NETWORK.activation_height(ZpNetworkUpgrade::Nu6_3),
        Some(1.into())
    );
}

#[test]
fn address_prefixes_and_coin_types_match() {
    let cases = [
        (
            NetworkType::Main,
            &MAINNET,
            mainnet::HRP_UNIFIED_ADDRESS,
            mainnet::HRP_UNIFIED_FVK,
            mainnet::HRP_UNIFIED_IVK,
            mainnet::COIN_TYPE,
        ),
        (
            NetworkType::Test,
            &TESTNET,
            testnet::HRP_UNIFIED_ADDRESS,
            testnet::HRP_UNIFIED_FVK,
            testnet::HRP_UNIFIED_IVK,
            testnet::COIN_TYPE,
        ),
        (
            NetworkType::Regtest,
            &REGTEST,
            regtest::HRP_UNIFIED_ADDRESS,
            regtest::HRP_UNIFIED_FVK,
            regtest::HRP_UNIFIED_IVK,
            regtest::COIN_TYPE,
        ),
    ];
    for (kind, params, ua, ufvk, uivk, coin_type) in cases {
        assert_eq!(ua, params.address_hrp, "{kind:?}");
        assert_eq!(ufvk, params.ufvk_hrp, "{kind:?}");
        assert_eq!(uivk, params.uivk_hrp, "{kind:?}");
        assert_eq!(coin_type, params.coin_type, "{kind:?}");
    }
}

#[test]
fn genesis_hashes_match() {
    let networks = [
        (Network::Mainnet, &MAINNET),
        (Network::new_default_testnet(), &TESTNET),
        (Network::new_regtest(Default::default()), &REGTEST),
    ];
    for (network, params) in networks {
        assert_eq!(
            network.genesis_hash().to_string(),
            params.genesis_hash,
            "{network}"
        );
    }
}

#[test]
fn regtest_block_subsidy_override_is_regtest_only() {
    use crate::{
        block::Height,
        parameters::{subsidy::block_subsidy, testnet::RegtestParameters},
    };

    let large = Network::new_regtest(RegtestParameters {
        block_subsidy: Some(25_000_000),
        ..Default::default()
    });
    let cash = |n: u64| -> crate::amount::Amount<crate::amount::NonNegative> {
        crate::amount::Amount::try_from(n * crate::amount::COIN as u64).unwrap()
    };
    assert_eq!(block_subsidy(Height(1), &large).unwrap(), cash(25_000_000));
    assert_eq!(block_subsidy(Height(7), &large).unwrap(), cash(25_000_000));
    assert_eq!(
        block_subsidy(Height(0), &large).unwrap(),
        cash(0),
        "genesis still pays nothing"
    );
    assert!(large.is_regtest());

    // Without the override, and on every other network, the CYPHES emission applies.
    for network in [
        Network::Mainnet,
        Network::new_default_testnet(),
        Network::new_regtest(Default::default()),
    ] {
        assert_eq!(network.regtest_block_subsidy(), None);
        assert_eq!(block_subsidy(Height(1), &network).unwrap(), cash(1_000));
    }

    // Above the 10 billion cap is rejected.
    let too_large = crate::parameters::testnet::Parameters::new_regtest(RegtestParameters {
        block_subsidy: Some(10_000_000_001),
        ..Default::default()
    });
    assert!(too_large.is_err());
}
