extern crate cov_specimen_multiple_tests;

fn main() {
}

#[test]
fn test2() {
    use cov_specimen_multiple_tests::ramsey;
    assert_eq!(ramsey(3, 9), 36);
}