//! Cross-platform methods to search for the system profiler, `cargo` and `rustc`.

use error::{ErrorKind, Result};
use utils::compare_naturally;

use glob::{MatchOptions, glob_with};

use std::env;
use std::ffi::{OsStr, OsString};
use std::fs::File;
use std::io::Read;
use std::path::{Path, PathBuf};

/// Glob patterns of the folders that may contain the compiler-rt profiler library `libclang_rt.profile*.a`.
const PROFILER_GLOB_PATTERNS: &[&str] = &[
    // macOS via Xcode
    "/Applications/Xcode.app/Contents/Developer/Toolchains/XcodeDefault.xctoolchain/usr/lib/clang/*/lib/darwin/",
    "/Library/Developer/CommandLineTools/usr/lib/clang/*/lib/darwin/",

    // Debian & Ubuntu "libclang-common-3.8-dev" package
    "/usr/lib/llvm-*/lib/clang/*/lib/linux/",

    // Fedora "compiler-rt" package OpenSUSE / OpenSUSE "llvm-clang" package
    "/usr/lib*/clang/*/lib/linux/",

    // macOS via Homebrew
    "/usr/local/opt/llvm/lib/clang/*/lib/darwin/",

    // LLVM installer on Windows
    r"C:\Program Files\LLVM\lib\clang\*\lib\windows\",
    r"C:\Program Files (x86)\LLVM\lib\clang\*\lib\windows\",

    // Android NDK on macOS, installed via Homebrew
    "/usr/local/share/android-sdk/ndk-bundle/toolchains/llvm/prebuilt/*/lib*/clang/*/lib/linux/",

    // Android NDK elsewhere
    "/opt/android-sdk/ndk-bundle/toolchains/llvm/prebuilt/*/lib*/clang/*/lib/linux/",
];


/// Obtains the expected name part of the compiler-rt profiler library for the specific target.
///
/// The compiler-rt profiler library is always named as `libclang_rt.profile*.a`, but the `*` part is target-specific.
/// For instance, on Linux and Windows x86-64 it is `"-x86_64"`, but for macOS it becomes `"_osx"`. This function finds
/// the `*` part when given the `target`.
///
/// # Errors
///
/// Returns [`NoDefaultProfilerLibrary`] if the `target` is unknown.
///
/// [`NoDefaultProfilerLibrary`]: ../error/enum.ErrorKind.html#variant.NoDefaultProfilerLibrary
fn profiler_name_part(target: &str) -> Result<&str> {
    Ok(match target {
        // iOS and macOS
        "aarch64-apple-ios" | "armv7-apple-ios" | "armv7s-apple-ios" => "_ios",
        "i386-apple-ios" | "x86_64-apple-ios" => "_iossim",
        "i686-apple-darwin" | "x86_64-apple-darwin" => "_osx",

        // Android
        "aarch64-linux-android" => "-aarch64-android",
        "arm-linux-androideabi" | "armv7-linux-androideabi" => "-arm-android",
        "i686-linux-android" => "-i686-android",
        "x86_64-linux-android" => "-x86_64-android",
        "mipsel-linux-android" => "-mipsel-android",
        "mips64el-linux-android" => "-mips64el-android",

        // Windows -- LLVM's installer provides -i386 packages.
        "i586-pc-windows-msvc" | "i686-pc-windows-msvc" => "-i386",

        // ARM with hard-float support
        "arm-unknown-linux-gnueabihf" | "arm-unknown-linux-musleabihf" | "armv7-unknown-linux-gnueabihf" | "armv7-unknown-linux-musleabihf" | "thumbv7em-none-eabihf" => "-armhf",

        // Everything else
        _ => {
            match target.split('-').next().unwrap_or("<no-architecture>") {
                "aarch64" => "-aarch64",
                "x86_64" => "-x86_64",
                "arm" | "armv5te" | "thumbv6m" | "thumbv7em" | "thumbv7m" => "-arm",
                "i586" => "-i386",
                "i686" => "-i686",
                "mips" => "-mips",
                "mipsel" => "-mipsel",
                "mips64" => "-mips64",
                "mips64el" => "-mips64el",
                "powerpc64" => "-powerpc64",
                "powerpc64le" => "-powerpc64le",
                "s390x" => "-s390x",
                _ => bail!(ErrorKind::NoDefaultProfilerLibrary),
            }
        },
    })
}

/// Locates the compiler-rt profiler library for the specific target.
///
/// The compiler-rt profiler library is always named as `libclang_rt.profile*.a`, where the `*` part is target-specific,
/// and the folder containing depends on the host.
///
/// This function returns the folder and name of the library, so they can be passed as `-L` and `-l` flags to `rustc`.
///
/// # Errors
///
/// Returns [`NoDefaultProfilerLibrary`] if the profiler library is not found.
///
/// [`NoDefaultProfilerLibrary`]: ../error/enum.ErrorKind.html#variant.NoDefaultProfilerLibrary
pub fn find_native_profiler_lib(target: &str) -> Result<(PathBuf, String)> {
    let part = profiler_name_part(target)?;
    let (prefix, suffix) = if target.ends_with("-msvc") {
        ("", ".lib")
    } else {
        ("lib", ".a")
    };

    let libname = ["clang_rt.profile", part].concat();
    let filename = [prefix, &libname, suffix].concat();

    let match_options = MatchOptions {
        case_sensitive: cfg!(not(windows)),
        require_literal_separator: true,
        require_literal_leading_dot: true,
    };
    for &glob_path in PROFILER_GLOB_PATTERNS {
        let pattern = glob_path.to_owned() + &filename;
        let paths = glob_with(&pattern, &match_options).expect("glob pattern");
        let path = paths
            .filter_map(|gr| match gr {
                Ok(path) => Some(path),
                Err(e) => {
                    debug!("cannot glob {}: {}", pattern, e);
                    None
                },
            })
            .max_by(|a, b| compare_naturally(a, b));
        if let Some(mut path) = path {
            path.pop();
            return Ok((path, libname));
        };
    }

    Err(ErrorKind::NoDefaultProfilerLibrary.into())
}

/// Splits the full path of a library to the folder and library name.
///
/// For instance `/usr/lib/libfoo.a` will be transformed to `("/usr/lib/", "foo")`. The result can be passed as `-L` and
/// `-l` flags to `rustc`.
///
/// # Errors
///
/// Returns [`InvalidProfilerLibraryPath`] if the file name cannot be encoded as UTF-8.
///
/// [`InvalidProfilerLibraryPath`]: ../error/enum.ErrorKind.html#variant.InvalidProfilerLibraryPath
pub fn split_profiler_lib(profiler: &Path) -> Result<(&Path, &str)> {
    let stem = profiler.file_stem().and_then(OsStr::to_str).ok_or(ErrorKind::InvalidProfilerLibraryPath)?;
    let libname = if profiler.extension() == Some(OsStr::new("a")) && stem.starts_with("lib") {
        &stem[3..]
    } else {
        stem
    };
    let lib_path = profiler.parent().unwrap_or_else(|| Path::new("."));
    Ok((lib_path, libname))
}

#[test]
#[cfg(not(windows))]
fn test_split_profiler_lib() {
    let p = Path::new("/usr/lib/libfoo.1.a");
    assert_eq!(split_profiler_lib(p).unwrap(), (Path::new("/usr/lib"), "foo.1"));
}

#[test]
#[cfg(windows)]
fn test_split_profiler_lib() {
    let p = Path::new(r"C:\Program Files (x86)\foo.1.lib");
    assert_eq!(split_profiler_lib(p).unwrap(), (Path::new(r"C:\Program Files (x86)"), "foo.1"));
}


/// Finds the path to `cargo`.
pub fn find_cargo() -> OsString {
    env::var_os("CARGO").unwrap_or_else(|| "cargo".into())
}


/// Finds the path to `rustc` or `rustdoc`.
///
/// This function will read the environment variable defined by `tool_name`, which should be the string `"RUSTC"` or
/// `"RUSTDOC"`. If the environment variable is not defined, it will try to read from the Cargo configuration at
/// `.cargo/config`.
pub fn find_rustc(tool_name: &str) -> String {
    if let Ok(rustc) = env::var(tool_name) {
        return rustc;
    }
    if let Ok(rustc) = find_rustc_via_cargo_config(tool_name) {
        return rustc;
    }
    tool_name.to_lowercase()
}

/// Finds the path to `rustc` or `rustdoc`.
///
/// This function will read the configuration at `.cargo/config`. The `tool_name` should be the
/// string `"RUSTC"` or `"RUSTDOC"`.
///
/// # Errors
///
/// * Returns [`NoRustc`] if the tool cannot be found by reading `.cargo/config` alone.
/// * Returns [`Io`] on I/O failure.
///
/// [`NoRustc`]: ../error/enum.ErrorKind.html#variant.NoRustc
/// [`Io`]: ../error/enum.ErrorKind.html#variant.Io
fn find_rustc_via_cargo_config(tool_name: &str) -> Result<String> {
    fn get_rustc_at_path(path: &Path, tool_name: &str) -> Result<String> {
        use toml::from_slice;
        let mut file = File::open(path)?;
        let mut content = Vec::new();
        file.read_to_end(&mut content)?;
        let config = from_slice::<CargoConfig>(&content)?;
        let build = config.build.ok_or(ErrorKind::NoRustc)?;
        let rustc = match tool_name {
            "RUSTC" => build.rustc,
            "RUSTDOC" => build.rustdoc,
            _ => unreachable!("unknown tool {}", tool_name),
        }.ok_or(ErrorKind::NoRustc)?;
        Ok(rustc.to_owned())
    }

    let mut base = env::current_dir()?;
    loop {
        base.push(".cargo");
        base.push("config");
        let rustc = get_rustc_at_path(&base, tool_name);
        if rustc.is_ok() {
            return rustc;
        }
        base.pop();
        base.pop();
        ensure!(base.pop(), ErrorKind::NoRustc);
    }
}

#[derive(Deserialize)]
struct CargoConfig<'a> {
    #[serde(borrow)]
    build: Option<CargoConfigBuild<'a>>,
}

#[derive(Deserialize)]
struct CargoConfigBuild<'a> {
    #[serde(borrow)]
    rustc: Option<&'a str>,
    #[serde(borrow)]
    rustdoc: Option<&'a str>,
}
