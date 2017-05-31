#[macro_use]
extern crate error_chain;
extern crate cov;
extern crate env_logger;
extern crate serde_json;

use cov::{Gcov, Interner, Result};

use std::env;
use std::io::stdout;

quick_main!(run);

fn run() -> Result<()> {
    env_logger::init().unwrap();

    let filename = env::args_os().nth(1).expect("filename");
    let mut interner = Interner::new();
    let parsed = Gcov::open(filename, &mut interner)?;
    serde_json::to_writer_pretty(stdout(), &interner.with(&parsed))?;
    Ok(())
}
