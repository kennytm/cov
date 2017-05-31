#[macro_use]
extern crate error_chain;
extern crate cov;
extern crate env_logger;
extern crate serde_json;

use cov::{Gcov, Graph, Interner, Result};

use std::env;
use std::io::stdout;

quick_main!(run);

fn run() -> Result<()> {
    env_logger::init().unwrap();

    let mut graph = Graph::default();
    let mut interner = Interner::new();
    for filename in env::args_os().skip(1) {
        let gcov = Gcov::open(filename, &mut interner)?;
        graph.merge(gcov)?;
    }
    graph.analyze();

    let coverage = graph.report();
    serde_json::to_writer_pretty(stdout(), &interner.with(&coverage))?;
    Ok(())
}
