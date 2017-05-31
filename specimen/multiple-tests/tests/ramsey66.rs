extern crate cov_specimen_multiple_tests;
use cov_specimen_multiple_tests::ramsey;

fn main() {}

#[test]
#[should_panic(expected="I don't know...")]
fn test() {
    assert_eq!(ramsey(6, 6), 115);
}
