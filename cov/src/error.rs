//! Errors related to the `cov` crate.
//!
//! Please see documentation of the [`error-chain` crate](https://docs.rs/error-chain/0.10.0/error_chain/) for detailed
//! usage.

use raw::{Ident, Type, Version};

use std::io;
use std::string::FromUtf8Error;

error_chain! {
    foreign_links {
        Io(io::Error) /** Wrapper of standard I/O error. */;
        FromUtf8(FromUtf8Error) /** Wrapper of UTF-8 decode error. */;
        Json(::serde_json::Error) #[cfg(feature="serde_json")] /** Wrapper of JSON error. */;
    }

    errors {
        /// Trying to read a file which is not GCNO/GCDA format.
        UnknownFileType(magic: u32) {
            description("unknown file type")
            display("unknown file type, magic 0x{:08x} not recognized", magic)
        }

        /// Version of a [`Gcov`] does not match that of the [`Graph`] when using [`merge()`].
        ///
        /// [`Gcov`]: ../raw/struct.Gcov.html
        /// [`Graph`]: ../graph/struct.Graph.html
        /// [`merge()`]: ../graph/struct.Graph.html#method.merge
        VersionMismatch(expected: Version, actual: Version) {
            description("version mismatch")
            display("version mismatch, existing graph has \"{}\", incoming file has \"{}\"", expected, actual)
        }

        /// Reached the end of a record when reading. Usually not fatal.
        Eof {
            description("encountered EOF record")
        }

        /// Encountered an unknown record.
        UnknownTag(tag: u32) {
            description("unknown record")
            display("unknown record, tag 0x{:08x} not recognized", tag)
        }

        /// Encountered an unknown block/arc flag.
        UnsupportedAttr(kind: &'static str, raw_flag: u32) {
            description("unsupported flags")
            display("unsupported {} flags 0x{:x}", kind, raw_flag)
        }

        /// The GCNO/GCDA is created for a GCC version that is not recognized by the `cov` crate.
        UnsupportedVersion(version: u32) {
            description("unsupported gcov version")
            display("unsupported gcov version 0x{:08x}", version)
        }

        /// The GCDA provides statistics of a function which cannot be found from the [`Graph`]. This error typically
        /// arises when merging a GCDA before its corresponding GCNO, or running an outdated version of program after
        /// the code has been recompiled (which generates a new GCNO).
        ///
        /// [`Graph`]: ../graph/struct.Graph.html
        MissingFunction(file_checksum: u32, ident: Ident) {
            description("missing function")
            display("function from *.gcda cannot be found in the *.gcno (checksum: {}, ident: {})", file_checksum, ident)
        }

        /// The GCNO provides information about a function which has already been merged into the [`Graph`]. The error
        /// typically arises when merging the same GCNO twice.
        ///
        /// [`Graph`]: ../graph/struct.Graph.html
        DuplicatedFunction(file_checksum: u32, ident: Ident) {
            description("duplicated function")
            display("the same function is added twice (checksum: {}, ident: {}), is the same *.gcno added twice?", file_checksum, ident)
        }

        /// The expected number of profilable arcs on the GCDA and GCNO differs.
        CountsMismatch(kind: &'static str, ty: Type, expected: usize, actual: usize) {
            description("counts mismatch")
            display("{0} counts mismatch on *.{3}, expecting {1} {0}, received {2} {0}", kind, expected, actual, ty)
        }
    }
}

//----------------------------------------------------------------------------------------------------------------------

/// A trait to log contextual information. When applied on an error value, a warning message will be printed out to
/// indicate an unexpected error.
pub trait At: Sized {
    /// Checks whether the error is caused by an unexpected EOF.
    fn is_eof(&self) -> bool;

    /// Checks whether a warning should be printed out.
    fn should_warn(&self) -> bool {
        !self.is_eof()
    }

    /// Marks the current error with file cursor information.
    fn at_cursor(self, cursor: u64) -> Self {
        if self.should_warn() {
            warn!("At file position {0} (0x{0:x}):", cursor)
        }
        self
    }

    /// Marks the current error with record index information.
    fn at_index(self, index: usize) -> Self {
        if self.should_warn() {
            warn!("At record index {}:", index)
        }
        self
    }

    /// Marks the current error with the previous file cursor information.
    fn before(self, cursor: u64) -> Self {
        self.at_cursor(cursor - 4)
    }
}

impl<T, E: At> At for ::std::result::Result<T, E> {
    fn is_eof(&self) -> bool {
        self.as_ref().err().map_or(false, E::is_eof)
    }

    fn should_warn(&self) -> bool {
        self.as_ref().err().map_or(false, E::should_warn)
    }
}

impl At for ErrorKind {
    fn is_eof(&self) -> bool {
        match *self {
            ErrorKind::Io(ref e) => e.is_eof(),
            ErrorKind::Eof => true,
            _ => false,
        }
    }
}

impl At for Error {
    fn is_eof(&self) -> bool {
        self.kind().is_eof()
    }
}

impl At for io::Error {
    fn is_eof(&self) -> bool {
        self.kind() == io::ErrorKind::UnexpectedEof
    }
}

impl At for FromUtf8Error {
    fn is_eof(&self) -> bool {
        false
    }
}
