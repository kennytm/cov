pub struct Fibonacci {
    a: u64,
    b: u64,
}

impl Fibonacci {
    pub fn new() -> Fibonacci {
        Fibonacci { a: 1, b: 1 }
    }
}

impl Iterator for Fibonacci {
    type Item = u64;
    fn next(&mut self) -> Option<u64> {
        let res = self.a;
        self.a = self.b;
        self.b += res;
        Some(res)
    }
}
