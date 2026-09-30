//! Randomised property tests for Proof of Work.

use proptest::prelude::*;

use crate::{
    block::Header,
    serialization::{ZcashDeserializeInto, ZcashSerialize},
    work::beamhash::Solution,
};

#[test]
fn beamhash_solution_roundtrip() {
    let _init_guard = zebra_test::init();

    proptest!(|(solution in any::<Solution>())| {
        let data = solution
            .zcash_serialize_to_vec()
            .expect("randomized solution should serialize");
        let solution2 = data
            .zcash_deserialize_into()
            .expect("randomized solution should deserialize");

        prop_assert_eq![solution, solution2];
    });
}

#[test]
fn header_roundtrip() {
    let _init_guard = zebra_test::init();

    proptest!(|(header in any::<Header>())| {
        let data = header.zcash_serialize_to_vec().expect("header should serialize");
        prop_assert_eq!(data.len(), Header::SERIALIZED_SIZE);
        let header2: Header = data.zcash_deserialize_into().expect("header should deserialize");
        prop_assert_eq![header, header2];
    });
}

#[test]
fn random_headers_do_not_validate() {
    let _init_guard = zebra_test::init();

    proptest!(|(header in any::<Header>())| {
        prop_assert!(header.solution.check(&header).is_err());
    });
}
