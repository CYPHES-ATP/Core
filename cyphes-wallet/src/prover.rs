//! A Sapling "prover" that refuses to prove.
//!
//! librustzcash's transaction builder takes Sapling provers even when a
//! transaction has no Sapling parts. CYPHES transactions never do: consensus
//! rejects Sapling, the wallet only pays Ironwood receivers, and change goes
//! to Ironwood. So instead of shipping Sapling's Groth16 parameters, this
//! prover fails if it is ever asked for a Sapling proof.

use sapling_crypto::{
    bundle::GrothProofBytes,
    circuit,
    keys::EphemeralSecretKey,
    prover::{OutputProver, SpendProver},
    value::{NoteValue, ValueCommitTrapdoor},
    Diversifier, MerklePath, PaymentAddress, ProofGenerationKey, Rseed,
};

/// A prover for transactions with no Sapling spends or outputs.
pub struct NoSapling;

const NO_SAPLING: &str = "CYPHES transactions never contain Sapling spends or outputs";

impl SpendProver for NoSapling {
    type Proof = ();

    fn prepare_circuit(
        _proof_generation_key: ProofGenerationKey,
        _diversifier: Diversifier,
        _rseed: Rseed,
        _value: NoteValue,
        _alpha: jubjub::Fr,
        _rcv: ValueCommitTrapdoor,
        _anchor: bls12_381::Scalar,
        _merkle_path: MerklePath,
    ) -> Option<circuit::Spend> {
        // Declines, so the builder reports an error instead of proving.
        None
    }

    fn create_proof<R: rand::RngCore>(&self, _circuit: circuit::Spend, _rng: &mut R) {
        unreachable!("{NO_SAPLING}")
    }

    fn encode_proof(_proof: ()) -> GrothProofBytes {
        unreachable!("{NO_SAPLING}")
    }
}

impl OutputProver for NoSapling {
    type Proof = ();

    fn prepare_circuit(
        _esk: &EphemeralSecretKey,
        _payment_address: PaymentAddress,
        _rcm: jubjub::Fr,
        _value: NoteValue,
        _rcv: ValueCommitTrapdoor,
    ) -> circuit::Output {
        // The wallet rejects every non-Ironwood recipient before building.
        unreachable!("{NO_SAPLING}")
    }

    fn create_proof<R: rand::RngCore>(&self, _circuit: circuit::Output, _rng: &mut R) {
        unreachable!("{NO_SAPLING}")
    }

    fn encode_proof(_proof: ()) -> GrothProofBytes {
        unreachable!("{NO_SAPLING}")
    }
}
