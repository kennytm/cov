#[macro_use]
extern crate error_chain;
extern crate cov;
extern crate env_logger;
extern crate serde_json;

use cov::{Gcov, Graph, Interner, Result};

use std::env;
use std::ffi::OsStr;
use std::io::stdout;

quick_main!(run);

fn run() -> Result<()> {
    env_logger::init().unwrap();

    let mut graph = Graph::default();
    let mut interner = Interner::new();
    let mut should_analyze = false;
    for filename in env::args_os().skip(1) {
        if filename == OsStr::new("--analyze") {
            should_analyze = true;
        } else {
            let gcov = Gcov::open(filename, &mut interner)?;
            graph.merge(gcov)?;
        }
    }
    if should_analyze {
        graph.analyze();
    }

    graph.write_dot(stdout())?;
    Ok(())
}
