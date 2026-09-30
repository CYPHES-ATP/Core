//! CYPHES networks as librustzcash consensus parameters.

use std::{fmt, str::FromStr};

use zcash_protocol::consensus::{BlockHeight, NetworkType, NetworkUpgrade, Parameters};

/// A CYPHES network.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Network {
    Mainnet,
    Testnet,
    Regtest,
}

impl Network {
    /// The CYPHES parameters for this network.
    pub fn params(self) -> &'static cyphes_params::NetworkParams {
        match self {
            Network::Mainnet => &cyphes_params::network::MAINNET,
            Network::Testnet => &cyphes_params::network::TESTNET,
            Network::Regtest => &cyphes_params::network::REGTEST,
        }
    }

    /// The node's default light-wallet gRPC endpoint on this machine.
    pub fn default_server(self) -> String {
        format!(
            "http://127.0.0.1:{}",
            self.params().default_wallet_grpc_port
        )
    }
}

impl Parameters for Network {
    fn network_type(&self) -> NetworkType {
        match self {
            Network::Mainnet => NetworkType::Main,
            Network::Testnet => NetworkType::Test,
            Network::Regtest => NetworkType::Regtest,
        }
    }

    /// CYPHES has no upgrade history: every upgrade through NU6.3 (Ironwood)
    /// is in force from block 1, on every network.
    fn activation_height(&self, nu: NetworkUpgrade) -> Option<BlockHeight> {
        #[allow(unreachable_patterns)]
        match nu {
            NetworkUpgrade::Overwinter
            | NetworkUpgrade::Sapling
            | NetworkUpgrade::Blossom
            | NetworkUpgrade::Heartwood
            | NetworkUpgrade::Canopy
            | NetworkUpgrade::Nu5
            | NetworkUpgrade::Nu6
            | NetworkUpgrade::Nu6_1
            | NetworkUpgrade::Nu6_2
            | NetworkUpgrade::Nu6_3 => Some(BlockHeight::from_u32(1)),
            _ => None,
        }
    }
}

impl FromStr for Network {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            "mainnet" => Ok(Network::Mainnet),
            "testnet" => Ok(Network::Testnet),
            "regtest" => Ok(Network::Regtest),
            other => Err(format!(
                "unknown network {other:?}: use mainnet, testnet or regtest"
            )),
        }
    }
}

impl fmt::Display for Network {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.params().name)
    }
}
