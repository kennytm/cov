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

        ForwardFailed(status: ExitStatus) {
            description("cargo failed")
            display("cargo exited with status {}", status)
        }
    }
}
