//! Derives a CYPHES Ironwood-only unified address and full viewing key.
//!
//! ```sh
//! cargo run -p zebra-chain --example cyphes_address -- <mainnet|testnet|regtest> [seed_hex]
//! ```
//!
//! Without `seed_hex` it uses the PUBLIC devnet seed below, which anyone can
//! recompute. Never send real value to addresses derived from it. For real
//! funds, use a wallet that generates and protects its own seed.

use orchard::keys::{FullViewingKey, Scope, SpendingKey};
use sha2::{Digest, Sha256};
use zcash_address::unified::{self, Encoding};
use zcash_protocol::consensus::NetworkType;

/// The public devnet seed: SHA-256 of this string.
const DEVNET_SEED_PHRASE: &str = "CYPHES public devnet seed. Never hold real value.";

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let (network, coin_type) = match args.first().map(String::as_str) {
        Some("mainnet") => (NetworkType::Main, cyphes_params::network::MAINNET.coin_type),
        Some("testnet") => (NetworkType::Test, cyphes_params::network::TESTNET.coin_type),
        Some("regtest") => (
            NetworkType::Regtest,
            cyphes_params::network::REGTEST.coin_type,
        ),
        _ => {
            eprintln!("usage: cyphes_address <mainnet|testnet|regtest> [seed_hex]");
            std::process::exit(2);
        }
    };
    let seed = match args.get(1) {
        Some(hex_seed) => hex::decode(hex_seed).expect("seed is hex"),
        None => {
            eprintln!("using the PUBLIC devnet seed: never send real value to this address");
            Sha256::digest(DEVNET_SEED_PHRASE.as_bytes()).to_vec()
        }
    };

    let sk = SpendingKey::from_zip32_seed(&seed, coin_type, zip32::AccountId::ZERO)
        .expect("valid seed and coin type");
    let fvk = FullViewingKey::from(&sk);
    let receiver = fvk.address_at(0u32, Scope::External).to_raw_address_bytes();

    let ua = unified::Address::try_from_items(vec![unified::Receiver::Orchard(receiver)])
        .expect("an Orchard receiver is a valid unified address");
    let ufvk = unified::Ufvk::try_from_items(vec![unified::Fvk::Orchard(fvk.to_bytes())])
        .expect("an Orchard key is a valid unified viewing key");

    println!("address: {}", ua.encode(&network));
    println!("ufvk:    {}", ufvk.encode(&network));
}
