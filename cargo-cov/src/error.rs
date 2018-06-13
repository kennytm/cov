//! Errors related to the `cargo-cov` crate.
//!
//! Please see documentation of the [`error-chain` crate](https://docs.rs/error-chain/0.10.0/error_chain/) for detailed
//! usage.

#![allow(renamed_and_removed_lints, unused_doc_comments)]
// ^ remove a release with https://github.com/rust-lang-nursery/error-chain/pull/247 is published.

use std::process::ExitStatus;

error_chain! {
    links {
        Cov(::cov::error::Error, ::cov::error::ErrorKind);
        Tera(::tera::Error, ::tera::ErrorKind);
    }

    foreign_links {
        TomlDe(::toml::de::Error);
        TomlSer(::toml::ser::Error);
        Io(::std::io::Error);
        Json(::serde_json::Error);
        WalkDir(::walkdir::Error);
    }

    errors {
        NoDefaultProfilerLibrary {
            description("no default profiler library for this target, please supply the --profiler option")
        }

        InvalidProfilerLibraryPath {
            description("the path set for --profiler is invalid, it should be the path of the static library itself (libclang_rt.profile-*.a)")
        }

        TargetDirectoryNotFound {
            description("cannot find target/ directory, please run `cargo update` and try again")
        }

        NoRustc {
            display(".cargo/config has no `build.rustc` key")
        }

        ForwardFailed(command: &'static str, status: ExitStatus) {
            description("command failed")
            display("{} exited with {}", command, status)
        }
    }
}
