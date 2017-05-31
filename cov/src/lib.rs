#![recursion_limit="128"] // needed for error_chain.

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
mod intern;
mod utils;
pub mod error;
pub mod raw;
pub mod reader;
pub mod graph;
pub mod report;

pub use error::{ErrorKind, Result};
pub use graph::Graph;
pub use intern::{Interner, Symbol};
pub use raw::Gcov;
pub use reader::Reader;
pub use report::Report;
