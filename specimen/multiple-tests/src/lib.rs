/// Produces the Ramsey number R(r, s).
///
/// ```rust
/// # use cov_specimen_multiple_tests::ramsey;
/// assert_eq!(ramsey(3, 3), 6);
/// ```
pub fn ramsey(r: u32, s: u32) -> u32 {
    if r > s {
        return ramsey(s, r);
    }

    match (r, s) {
        (1, _) => 1,
        (2, s) => s,
        (3, 3) => 6,
        (3, 4) => 9,
        (3, 5) => 14,
        (3, 6) => 18,
        (3, 7) => 23,
        (3, 8) => 28,
        (3, 9) => 36,
        (4, 4) => 18,
        (4, 5) => 25,
        _ => panic!("I don't know..."),
    }
}

#[test]
fn test() {
    assert_eq!(ramsey(1, 10000), 1);
    assert_eq!(ramsey(10000, 2), 10000);
}