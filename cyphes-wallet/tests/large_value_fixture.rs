//! Synthetic evidence for values near the 10 billion CASH cap.
//!
//! Builds an Ironwood coinbase-style bundle that creates one note of
//! 9,999,000,000 CASH, proves it with the unmodified post-NU6.3 circuit,
//! signs it, and verifies the proof and the binding signature. A second test
//! spends that note to another address and verifies the spend's proof and
//! every signature, as a node's batch validator does.
//!
//! This is a cryptographic fixture, not a mined note: nothing here touches a
//! chain. The money cycle (`tests/money_cycle.rs`) mines, receives and spends a
//! real 22 million CASH note; this test covers values no test chain can mine
//! quickly, and shows the forked `zcash_protocol` accepts them where Zcash's
//! 21 million `MAX_MONEY` would not.

use incrementalmerkletree::{Hashable, Level};
use orchard::{
    builder::{Builder, BundleType},
    bundle::{BatchValidator, BundleVersion, Flags},
    circuit::{ProvingKey, VerifyingKey},
    keys::{FullViewingKey, Scope, SpendAuthorizingKey, SpendingKey},
    tree::{Anchor, MerkleHashOrchard, MerklePath},
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

#[test]
fn ironwood_note_just_under_the_cap_is_spent() {
    let value = 9_999_000_000 * COIN;
    let fee = 10_000; // 0.0001 CASH
    let version = BundleVersion::ironwood_v3();

    let sk = SpendingKey::from_bytes([7; 32]).expect("valid spending key");
    let fvk = FullViewingKey::from(&sk);
    let owner = fvk.address_at(0u32, Scope::External);
    let payee = FullViewingKey::from(&SpendingKey::from_bytes([9; 32]).expect("valid key"))
        .address_at(0u32, Scope::External);

    // Create the note, and find it the way a wallet does: by trial decryption.
    let mut builder = Builder::new(
        BundleType::Coinbase,
        version,
        Flags::SPENDS_DISABLED,
        Anchor::empty_tree(),
    )
    .expect("valid builder");
    builder
        .add_output(None, owner, NoteValue::from_raw(value), [0; 512])
        .expect("output");
    let (created, _) = builder
        .build::<i64>(OsRng)
        .expect("build")
        .expect("non-empty bundle");
    let (index, _, note, _, _) = created
        .decrypt_outputs_with_keys(&[fvk.to_ivk(Scope::External)])
        .pop()
        .expect("the owner decrypts the note");
    assert_eq!(note.value().inner(), value);

    // The note is the first leaf of an otherwise empty commitment tree.
    let path = MerklePath::from_parts(
        0,
        std::array::from_fn(|level| MerkleHashOrchard::empty_root(Level::from(level as u8))),
    );
    let anchor = path.root(*created.actions()[index].cmx());

    // Spend all of it: the payee receives everything but the fee.
    let mut builder = Builder::new(
        BundleType::DEFAULT,
        version,
        version.default_flags(),
        anchor,
    )
    .expect("valid builder");
    builder.add_spend(fvk, note, path).expect("spend");
    builder
        .add_output(None, payee, NoteValue::from_raw(value - fee), [0; 512])
        .expect("output");
    let (bundle, _) = builder
        .build::<i64>(OsRng)
        .expect("build")
        .expect("non-empty bundle");
    // Only the fee leaves the Ironwood pool.
    assert_eq!(*bundle.value_balance(), fee as i64);

    let pk = ProvingKey::build(version.circuit_version());
    let vk = VerifyingKey::build(version.circuit_version());
    let sighash = [0x24; 32];
    let bundle = bundle
        .create_proof(&pk, OsRng)
        .expect("prove")
        .apply_signatures(OsRng, sighash, &[SpendAuthorizingKey::from(&sk)])
        .expect("sign");

    // The proof, the spend authorization signature and the binding signature,
    // checked together as Zebra's verifier does.
    let mut validator = BatchValidator::new(&vk);
    validator
        .add_bundle(&bundle, sighash)
        .expect("supported bundle");
    assert!(validator.validate(OsRng), "the spend verifies");

    // Control: the same bundle under another transaction's sighash fails.
    let mut validator = BatchValidator::new(&vk);
    validator
        .add_bundle(&bundle, [0x25; 32])
        .expect("supported bundle");
    assert!(
        !validator.validate(OsRng),
        "signatures bind the spend to its transaction"
    );
}
