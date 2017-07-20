//! `cov` is a GCNO/GCDA parser. GCNO/GCDA are source code coverage file formats produced by GCC and LLVM-based
//! compilers, including `rustc`.
//!
//! GCNO (gcov notes) files are created by rustc when given the `-Z profile` flag. The GCNO files encode the structure
//! of every function in the program, known as the [control-flow graph (CFG)][cfg]. The GCNO also contains the filename
//! and line number of each node in the CFG.
//!
//! GCDA (gcov data) files are created when running the code program produced with `-Z profile` flag. For every edge in
//! the CFG, the GCDA stores how many times this edge has been taken.
//!
//! Combining the statistics in GCDA and source information in GCNO, coverage tools can generate a branch coverage
//! report.
//!
//! ## Examples
//!
//! GCNO and GCDA have a similar format, and are parsed using the same [`Reader`] class. The result is the [`Gcov`]
//! structure. Complex projects will typically produce multiple GCNO and GCDA files. The statistics can be all merged
//! into a single [`Graph`] class for analysis. Finally, an export-friendly [`Report`] structure can be derived from the
//! `Graph`, to make it easy for creating a human-readable HTML report or generate data for third-party coverage
//! collection services.
//!
//! The typical usage is like:
//!
//! ```rust
//! extern crate cov;
//! extern crate serde_json;
//! use cov::{Gcov, Graph, Interner, SerializeWithInterner};
//!
//! # fn main() { run().unwrap(); }
//! # fn run() -> cov::Result<()> {
//! let mut interner = Interner::default();
//! let mut graph = Graph::default();
//!
//! // merge the coverage statistics.
//! // note: merge all gcno before gcda.
//! graph.merge(Gcov::open("test-data/trivial.clang/x.gcno", &mut interner)?)?;
//! graph.merge(Gcov::open("test-data/trivial.rustc/x.gcno", &mut interner)?)?;
//!
//! graph.merge(Gcov::open("test-data/trivial.clang/x.gcda", &mut interner)?)?;
//! graph.merge(Gcov::open("test-data/trivial.rustc/x.gcda", &mut interner)?)?;
//!
//! // analyze the graph (if you skip this step, the report will be empty)
//! graph.analyze();
//!
//! // produce the report.
//! let report = graph.report();
//!
//! // serialize the report into json.
//! println!("{}", serde_json::to_string_pretty(&report.with_interner(&interner))?);
//! # Ok(()) }
//! ```
//!
//! [cfg]: https://en.wikipedia.org/wiki/Control_flow_graph
//! [`Reader`]: ./reader/struct.Reader.html
//! [`Gcov`]: ./raw/struct.Gcov.html
//! [`Graph`]: ./graph/struct.Graph.html
//! [`Report`]: ./report/struct.Report.html

#![recursion_limit = "128"] // needed for error_chain.
#![doc(html_root_url = "https://docs.rs/cov/0.1.0")]

#![cfg_attr(feature = "cargo-clippy", warn(warnings, clippy_pedantic))]
#![cfg_attr(feature = "cargo-clippy", allow(missing_docs_in_private_items, use_debug, cast_possible_truncation))]

#[macro_use]
extern crate error_chain;
#[macro_use]
extern crate bitflags;
#[macro_use]
extern crate log;
#[cfg(feature = "serde")]
#[macro_use]
extern crate serde;
#[cfg(feature = "serde_json")]
extern crate serde_json;
extern crate byteorder;
extern crate petgraph;
extern crate fixedbitset;
extern crate num_traits; // required for shawshank
extern crate shawshank;

#[macro_use]
pub mod intern;
#[cfg(feature = "serde")]
pub mod deserializer;
mod utils;
pub mod error;
pub mod raw;
pub mod reader;
pub mod graph;
pub mod report;

#[cfg(feature = "serde")]
pub use deserializer::with_interner as deserializer_with_interner;
pub use error::{ErrorKind, Result};
pub use graph::Graph;
pub use intern::{Interner, Symbol};
#[cfg(feature = "serde")]
pub use intern::SerializeWithInterner;
pub use raw::Gcov;
pub use report::Report;
pub use utils::IntoStringLossy;
