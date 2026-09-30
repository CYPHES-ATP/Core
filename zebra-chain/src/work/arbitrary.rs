use super::*;

use proptest::{collection::vec, prelude::*};

impl Arbitrary for beamhash::Solution {
    type Parameters = ();

    fn arbitrary_with(_args: Self::Parameters) -> Self::Strategy {
        (vec(any::<u8>(), beamhash::SOLUTION_LEN))
            .prop_map(|v| Self::from_bytes(&v).expect("vector has the solution length"))
            .boxed()
    }

    type Strategy = BoxedStrategy<Self>;
}
