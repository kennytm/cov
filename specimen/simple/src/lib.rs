/// Performs some silly computation.
///
/// ```rust
/// assert_eq!(cov_specimen_simple::compute(5, "/", 3), 1);
/// ```
///
/// # Panics
///
/// Panics when operator is unknown.
///
/// ```rust,should_panic
/// cov_specimen_simple::compute(5, "??", 3); // panics
/// ```
pub fn compute(a: i64, op: &str, b: i64) -> i64 {
    match op {
        "+" => a + b,
        "-" => a - b,
        "*" => a * b,
        "/" => a / b,
        "^" => {
            let mut res = 1;
            for _ in 0 .. b {
                res *= a;
            }
            res
        }
        "max" => if a > b {
            a
        } else {
            b
        },
        "min" => if a < b {
            a
        } else {
            b
        },
        _ => panic!("unsupported operation {}", op),
    }
}

#[test]
fn test_plus() {
    assert_eq!(compute(1, "+", 2), 3);
    assert_eq!(compute(4, "+", 5), 9);
}

#[test]
fn test_max_min() {
    assert_eq!(compute(12, "max", 8), 12);
    assert_eq!(compute(5, "max", 5), 5);
    assert_eq!(compute(2, "min", 1), 1);
}

#[test]
#[should_panic(expected="unsupported operation")]
fn test_unsupported_op() {
    compute(123, "&", 567);
}

#[test]
fn test_pow() {
    assert_eq!(compute(-6, "^", 5), -7776);
}
