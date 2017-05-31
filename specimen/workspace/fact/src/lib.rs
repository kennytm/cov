pub fn factorial(a: u64) -> u64 {
    if a == 0 {
        1
    } else {
        a * factorial(a - 1)
    }
}
