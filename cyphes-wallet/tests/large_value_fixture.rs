//! Synthetic evidence for values near the 10 billion CASH cap.
//!
//! Builds an Ironwood coinbase-style bundle that creates one note of
//! 9,999,000,000 CASH, proves it with the unmodified post-NU6.3 circuit,
//! signs it, and verifies the proof and the binding signature.
//!
//! This is a cryptographic fixture, not a mined note: nothing here touches a
//! chain. The money cycle (`tests/money_cycle.rs`) mines, receives and spends a
//! real 22 million CASH note; this test covers values no test chain can mine
//! quickly, and shows the forked `zcash_protocol` accepts them where Zcash's
//! 21 million `MAX_MONEY` would not.

use orchard::{
    builder::{Builder, BundleType},
    bundle::{BundleVersion, Flags},
    circuit::{ProvingKey, VerifyingKey},
    keys::{FullViewingKey, Scope, SpendingKey},
    tree::Anchor,
    value::NoteValue,
};
use rand::rngs::OsRng;
use zcash_protocol::value::{ZatBalance, Zatoshis, MAX_MONEY};

const COIN: u64 = 100_000_000;

#[test]
fn ironwood_note_just_under_the_cap_proves_and_verifies() {
    let value = 9_999_000_000 * COIN;
    assert!(value > 21_000_000 * COIN && value <= MAX_MONEY);

    let sk = SpendingKey::from_bytes([7; 32]).expect("valid spending key");
    let recipient = FullViewingKey::from(&sk).address_at(0u32, Scope::External);

    let version = BundleVersion::ironwood_v3();
    // Coinbase-style: outputs only, no spends.
    let flags = Flags::SPENDS_DISABLED;
    let mut builder = Builder::new(BundleType::Coinbase, version, flags, Anchor::empty_tree())
        .expect("valid builder");
    builder
        .add_output(None, recipient, NoteValue::from_raw(value), [0; 512])
        .expect("output");
    let (bundle, _) = builder
        .build::<i64>(OsRng)
        .expect("build")
        .expect("non-empty bundle");

    // The bundle moves exactly `value` into the Ironwood pool.
    assert_eq!(*bundle.value_balance(), -(value as i64));

    let pk = ProvingKey::build(version.circuit_version());
    let vk = VerifyingKey::build(version.circuit_version());
    let sighash = [0x42; 32];
    let bundle = bundle
        .create_proof(&pk, OsRng)
        .expect("prove")
        .apply_signatures(OsRng, sighash, &[])
        .expect("sign");

    bundle
        .verify_proof(&vk)
        .expect("the circuit accepts the proof");
    bundle
        .binding_validating_key()
        .verify(&sighash, bundle.authorization().binding_signature())
        .expect("value is conserved: the binding signature verifies");

    // The CYPHES zcash_protocol accepts this value balance and note value;
    // Zcash's 21 million MAX_MONEY would reject both.
    assert!(ZatBalance::from_i64(*bundle.value_balance()).is_ok());
    assert!(Zatoshis::from_u64(value).is_ok());
    assert!(value > 21_000_000 * COIN);
}
