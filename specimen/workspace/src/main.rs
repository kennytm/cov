extern crate cov_specimen_workspace_fib;
extern crate cov_specimen_workspace_fact;

use cov_specimen_workspace_fib::Fibonacci;
use cov_specimen_workspace_fact::factorial;

fn main() {
    for i in Fibonacci::new() {
        println!("{}", factorial(i));
    }
}
