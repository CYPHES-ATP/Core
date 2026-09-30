//! Constants for the CYPHES main network.
//!
//! CYPHES fork: every prefix and HRP differs from Zcash's, so no Zcash address
//! or key parses as a CYPHES one. CYPHES only uses unified addresses with an
//! Ironwood receiver; the Sapling, Sprout and transparent encodings exist
//! because other librustzcash crates reference them.

/// The CYPHES mainnet coin type: provisional, "CASH" in ASCII, to be
/// registered in [SLIP 44].
///
/// [SLIP 44]: https://github.com/satoshilabs/slips/blob/master/slip-0044.md
pub const COIN_TYPE: u32 = 0x4341_5348;

/// The HRP for a Bech32-encoded mainnet Sapling [`ExtendedSpendingKey`].
///
/// Defined in [ZIP 32].
///
/// [`ExtendedSpendingKey`]: https://docs.rs/sapling-crypto/latest/sapling_crypto/zip32/struct.ExtendedSpendingKey.html
/// [ZIP 32]: https://github.com/zcash/zips/blob/main/zips/zip-0032.rst
pub const HRP_SAPLING_EXTENDED_SPENDING_KEY: &str = "secret-extended-key-cyphes-main";

/// The HRP for a Bech32-encoded mainnet [`ExtendedFullViewingKey`].
///
/// Defined in [ZIP 32].
///
/// [`ExtendedFullViewingKey`]: https://docs.rs/sapling-crypto/latest/sapling_crypto/zip32/struct.ExtendedFullViewingKey.html
/// [ZIP 32]: https://github.com/zcash/zips/blob/main/zips/zip-0032.rst
pub const HRP_SAPLING_EXTENDED_FULL_VIEWING_KEY: &str = "cyphxviews";

/// The HRP for a Bech32-encoded mainnet Sapling [`PaymentAddress`].
///
/// Defined in section 5.6.4 of the [Zcash Protocol Specification].
///
/// [`PaymentAddress`]: https://docs.rs/sapling-crypto/latest/sapling_crypto/struct.PaymentAddress.html
/// [Zcash Protocol Specification]: https://github.com/zcash/zips/blob/main/rendered/protocol/protocol.pdf
pub const HRP_SAPLING_PAYMENT_ADDRESS: &str = "cyphs";

/// The prefix for a Base58Check-encoded mainnet Sprout address.
///
/// Defined in the [Zcash Protocol Specification section 5.6.3][sproutpaymentaddrencoding].
///
/// [sproutpaymentaddrencoding]: https://zips.z.cash/protocol/protocol.pdf#sproutpaymentaddrencoding
pub const B58_SPROUT_ADDRESS_PREFIX: [u8; 2] = [0x16, 0x5c];

/// The prefix for a Base58Check-encoded DER-encoded mainnet [`SecretKey`], as specified via the
/// bitcoin-derived [`EncodeSecret`] format function.
///
/// [`SecretKey`]: https://docs.rs/secp256k1/latest/secp256k1/struct.SecretKey.html
/// [`EncodeSecret`]: https://github.com/zcash/zcash/blob/1f1f7a385adc048154e7f25a3a0de76f3658ca09/src/key_io.cpp#L298
pub const B58_SECRET_KEY_PREFIX: [u8; 1] = [0x80];

/// The prefix for a Base58Check-encoded mainnet [`PublicKeyHash`].
///
/// [`PublicKeyHash`]: https://docs.rs/zcash_transparent/latest/zcash_transparent/address/enum.TransparentAddress.html
pub const B58_PUBKEY_ADDRESS_PREFIX: [u8; 2] = [0x1c, 0x48];

/// The prefix for a Base58Check-encoded mainnet [`ScriptHash`].
///
/// [`ScriptHash`]: https://docs.rs/zcash_transparent/latest/zcash_transparent/address/enum.TransparentAddress.html
pub const B58_SCRIPT_ADDRESS_PREFIX: [u8; 2] = [0x1c, 0x4d];

/// The HRP for a Bech32m-encoded mainnet [ZIP 320] TEX address.
///
/// [ZIP 320]: https://zips.z.cash/zip-0320
pub const HRP_TEX_ADDRESS: &str = "cyphtex";

/// The HRP for a Bech32m-encoded mainnet Unified Address.
///
/// Defined in [ZIP 316][zip-0316].
///
/// [zip-0316]: https://zips.z.cash/zip-0316
pub const HRP_UNIFIED_ADDRESS: &str = "cyph";

/// The HRP for a Bech32m-encoded mainnet Unified FVK.
///
/// Defined in [ZIP 316][zip-0316].
///
/// [zip-0316]: https://zips.z.cash/zip-0316
pub const HRP_UNIFIED_FVK: &str = "cyphview";

/// The HRP for a Bech32m-encoded mainnet Unified IVK.
///
/// Defined in [ZIP 316][zip-0316].
///
/// [zip-0316]: https://zips.z.cash/zip-0316
pub const HRP_UNIFIED_IVK: &str = "cyphivk";
