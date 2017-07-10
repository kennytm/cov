#[macro_use]
extern crate clap;
#[macro_use]
extern crate error_chain;
extern crate cov;
extern crate env_logger;
extern crate serde_json;

use cov::{Gcov, Graph, Interner, Result};
use cov::intern::UNKNOWN_SYMBOL;

use std::io::stdout;

quick_main!(run);

fn run() -> Result<()> {
    env_logger::init().unwrap();

    let matches = clap_app!(graphviz =>
        (@arg analyze: -a --analyze "Produce graph after analysis")
        (@arg filter: -f --filter +takes_value "Only produce graphs from this source file")
        (@arg files: <FILE>... "*.gcno and *.gcda files to form the graph")
    ).get_matches();

    let mut graph = Graph::default();
    let mut interner = Interner::new();
    let should_analyze = matches.is_present("analyze");
    for filename in matches.values_of_os("files").expect("files") {
        let gcov = Gcov::open(filename, &mut interner)?;
        graph.merge(gcov)?;
    }
    if should_analyze {
        graph.analyze();
    }

    let filter = match matches.value_of("filter") {
        None => UNKNOWN_SYMBOL,
        Some(s) => interner.intern(s),
    };
    graph.write_dot(filter, stdout())?;
    Ok(())
}
