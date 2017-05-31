extern crate cov_specimen_multiple_tests;
use cov_specimen_multiple_tests::ramsey;

fn main() {
    assert_eq!(ramsey(4, 4), 18);
}
