//! Coverage report.
//!
//! The [`Report`] structure contains format-independent information about the coverage of every line and branch. It can
//! be easily serialized via serde for human-readable report generation, or transformation to other format consumed by
//! external services.
//!
//! [`Report`]: ./struct.Report.html

#[cfg(feature = "serde")]
use intern::{Interner, SerializeWithInterner};
use intern::Symbol;
use raw::{ArcAttr, BlockAttr};
use utils::tuple_4_add;

#[cfg(feature = "serde")]
use serde::{Serialize, Serializer};

use std::collections::{BTreeMap, HashMap};

derive_serialize_with_interner! {
    /// A coverage report, generated from a [`Graph`].
    ///
    /// [`Graph`]: ../graph/struct.Graph.html
    #[derive(Clone, PartialEq, Eq, Debug, Default)]
    #[cfg_attr(feature="serde", derive(Serialize))]
    pub struct Report {
        /// Files in the report.
        pub files: HashMap<Symbol, File>,
    }
}

derive_serialize_with_interner! {
    /// Coverage information about a source file.
    #[derive(Clone, PartialEq, Eq, Hash, Debug, Default)]
    #[cfg_attr(feature="serde", derive(Serialize))]
    pub struct File {
        /// Lines in the file.
        pub lines: BTreeMap<u32, Line>,

        /// Functions in the file.
        pub functions: Vec<Function>,
    }
}

impl File {
    /// Produces a summary of the current file.
    pub fn summary(&self) -> FileSummary {
        let lines_count = self.lines.len();
        let lines_covered = self.lines.values().filter(|line| line.count > 0).count();
        let functions_count = self.functions.len();
        let (branches_count, branches_executed, branches_taken, functions_called) = self.functions
            .iter()
            .map(|f| {
                let s = &f.summary;
                (s.branches_count, s.branches_executed, s.branches_taken, (s.entry_count > 0) as usize)
            })
            .fold((0, 0, 0, 0), tuple_4_add);
        FileSummary {
            lines_count,
            lines_covered,
            branches_count,
            branches_executed,
            branches_taken,
            functions_count,
            functions_called,
        }
    }
}

derive_serialize_with_interner! {
    /// Coverage information about a source line of code.
    #[derive(Clone, PartialEq, Eq, Hash, Debug, Default)]
    #[cfg_attr(feature="serde", derive(Serialize))]
    pub struct Line {
        /// Number of times this line is executed.
        ///
        /// This is the number of times all branches targeting the basic block containing this line has been taken.
        pub count: u64,

        /// Attributes associated with this line.
        pub attr: BlockAttr,

        /// List of branches this line will lead to.
        pub branches: Vec<Branch>,
    }
}

derive_serialize_with_interner! {
    /// Coverage information about a branch.
    #[derive(Copy, Clone, PartialEq, Eq, Hash, Debug, Default)]
    #[cfg_attr(feature="serde", derive(Serialize))]
    pub struct Branch {
        /// Number of times this branch is taken.
        pub count: u64,

        /// Attributes associated with this branch.
        pub attr: ArcAttr,

        /// The target filename of this branch.
        pub filename: Symbol,

        /// The line number of the target of this branch. Zero if missing.
        pub line: u32,

        /// The column number of the target of this branch. Zero if missing.
        pub column: u32,
    }
}

derive_serialize_with_interner! {
    /// Coverage information about a function.
    #[derive(Copy, Clone, PartialEq, Eq, Hash, Debug, Default)]
    #[cfg_attr(feature="serde", derive(Serialize))]
    pub struct Function {
        /// Name of the function.
        pub name: Symbol,

        /// The line number where this function is defined. Zero if missing.
        pub line: u32,

        /// The column number where this function is defined. Zero if missing.
        pub column: u32,

        /// Summary about this function.
        pub summary: FunctionSummary,
    }
}

/// Statistical summary of a function.
#[derive(Copy, Clone, PartialEq, Eq, Hash, Debug, Default)]
#[cfg_attr(feature = "serde", derive(Serialize))]
pub struct FunctionSummary {
    /// Number of basic blocks in the function (excluding the enter- and exit-blocks).
    pub blocks_count: usize,

    /// Number of basic blocks that has been executed (having non-zero count).
    pub blocks_executed: usize,

    /// How many times the function is called.
    pub entry_count: u64,

    /// How many times the function has returned.
    pub exit_count: u64,

    /// Number of conditional branches in the function.
    pub branches_count: usize,

    /// Number of conditional basic blocks that has been executed.
    pub branches_executed: usize,

    /// Number of branches that has been taken.
    pub branches_taken: usize,
}

/// Statistical summary of a file.
#[derive(Copy, Clone, PartialEq, Eq, Hash, Debug, Default)]
#[cfg_attr(feature = "serde", derive(Serialize))]
pub struct FileSummary {
    /// Number of lines that can be profiled.
    pub lines_count: usize,

    /// Number of lines that has been covered.
    pub lines_covered: usize,

    /// Number of conditional branches in functions defined in this file.
    pub branches_count: usize,

    /// Number of conditional basic blocks that has been executed.
    pub branches_executed: usize,

    /// Number of branches that has been taken.
    pub branches_taken: usize,

    /// Number of functions defined in this file.
    pub functions_count: usize,

    /// Number of functions that has been called.
    pub functions_called: usize,
}

derive_serialize_with_interner! {
    direct: FunctionSummary, FileSummary
}
