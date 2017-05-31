//! Errors related to the `cov` crate.

use raw::{Ident, Type, Version};

use std::io;
use std::string::FromUtf8Error;

error_chain! {
    foreign_links {
        Io(io::Error);
        FromUtf8(FromUtf8Error);
        Json(::serde_json::Error) #[cfg(feature="serde_json")];
    }

    errors {
        UnknownFileType(magic: u32) {
            description("unknown file type")
            display("unknown file type, magic 0x{:08x} not recognized", magic)
        }

        ChecksumMismatch(kind: &'static str) {
            description("checksum mismatch")
            display("{} checksum mismatch between *.gcno and *.gcda", kind)
        }

        VersionMismatch(expected: Version, actual: Version) {
            description("version mismatch")
            display("version mismatch, *.gcno has \"{}\", *.gcda has \"{}\"", expected, actual)
        }

        Eof {
            description("encountered EOF record")
        }

        UnknownTag(tag: u32) {
            description("unknown record")
            display("unknown record, tag 0x{:08x} not recognized", tag)
        }

        UnsupportedAttr(kind: &'static str, raw_flag: u32) {
            description("unsupported flags")
            display("unsupported {} flags 0x{:x}", kind, raw_flag)
        }

        UnsupportedVersion(version: u32) {
            description("unsupported gcov version")
            display("unsupported gcov version 0x{:08x}", version)
        }

        WrongFileType(expected: Type, actual: Type, purpose: &'static str) {
            description("wrong gcov file type")
            display("wrong type file, expecting *.{} for {}, received a *.{}", expected, purpose, actual)
        }

        MissingFunction(file_checksum: u32, ident: Ident) {
            description("missing function")
            display("function from *.gcda cannot be found in the *.gcno (checksum: {}, ident: {})", file_checksum, ident)
        }

        DuplicatedFunction(file_checksum: u32, ident: Ident) {
            description("duplicated function")
            display("the same function is added twice (checksum: {}, ident: {}), is the same *.gcno added twice?", file_checksum, ident)
        }

        UnexpectedRecordType(expected: &'static str, actual: &'static str) {
            description("unexpected record type")
            display("unexpected record type, expecting {}, found {}", expected, actual)
        }

        CountsMismatch(kind: &'static str, ty: Type, expected: usize, actual: usize) {
            description("counts mismatch")
            display("{0} counts mismatch on *.{3}, expecting {1} {0}, received {2} {0}", kind, expected, actual, ty)
        }

        NoTemplate {
            description("no template for rendering")
        }
    }
}

#[cfg(feature = "handlebars")]
impl From<::handlebars::TemplateFileError> for Error {
    fn from(e: ::handlebars::TemplateFileError) -> Error {
        use handlebars::TemplateFileError::*;
        match e {
            TemplateError(e) => ErrorKind::HbTemplate(e),
            IOError(e, _) => ErrorKind::Io(e),
        }.into()
    }
}

//----------------------------------------------------------------------------------------------------------------------

/// A trait to log contextual information. When applied on an error value, a warning message will be printed out to
/// indicate an unexpected error.
pub trait At: Sized {
    /// Checks whether the error is caused by an unexpected EOF.
    fn is_eof(&self) -> bool;

    fn should_warn(&self) -> bool {
        !self.is_eof()
    }

    fn at_cursor(self, cursor: u64) -> Self {
        if self.should_warn() {
            warn!("At file position {0} (0x{0:x}):", cursor)
        }
        self
    }

    fn at_index(self, index: usize) -> Self {
        if self.should_warn() {
            warn!("At record index {}:", index)
        }
        self
    }

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
