//! Shims for `rustc`, `rustdoc` and test runner.
//!
//! When using `cargo cov` to compile or run a test, it will instruct `cargo` to callback to the `cargo-cov` executable
//! instead of running `rustc`, `rustdoc` or the test program directly. The `cargo-cov` will perform some operations
//! before and after forwarding to the underlying program to ensure the coverage reports are collected correctly.
//!
//! `rustc` flags
//! -------------
//!
//! The shim for `rustc` is executed by running `cargo-cov rustc-shim.bat <args>`. If the crate to be built is an
//! external crate, nothing will be modified. Otherwise, the shim will insert several flags like `-Zprofile` to enable
//! coverage. The different handling between workspace and external crates is the reason why `RUSTFLAGS` is not used.
//!
//! GCNO and GCDA saving
//! --------------------
//!
//! After `rustc`, `rustdoc` (doc tests) and the test programs are executed, a GCNO or GCDA file will be produced. The
//! shim will then immediately scan the build directory and move the file inside `target/cov/build/{gcno,gcda}` under a
//! unique new name.
//!
//! The renaming is necessary for non-nightly Rust where `-Zprofile` is not supported. The GCNO/GCDA produced will be
//! named after the crate, which both the doc-test and normal test coincide (`-Zprofile` fixes the problem by including
//! the hash as well). This will cause one GCNO to overwrite another, and GCDA-merge will produce a corrupt report.
//! `cargo cov` works-around this by moving these files to a unique location as soon as they are generated.

use argparse::is_rustc_compiling_local_crate;
use error::{Result, ResultExt};
use utils::{CommandExt, join_2, parent_3};

use fs2::FileExt;
use rand::{Rng, thread_rng};
use walkdir::WalkDir;

use std::env;
use std::ffi::{OsStr, OsString};
use std::fs::{File, rename};
use std::path::Path;
use std::process::Command;

/// Builds a crate by forwarding `args` to `rustc`.
///
/// This function requires several environment variables to be set, otherwise it will panic.
///
/// | Environment variable | Meaning |
/// |----------------------|---------|
/// | `COV_RUSTC` | Path to `rustc` executable |
/// | `COV_BUILD_PATH` | Path to `target/cov/build/` of the workspace |
/// | `COV_PROFILER_LIB_PATH` | Path to folder containing `libclang_rt.profile*.a`, or the string `"@native"` |
/// | `COV_PROFILER_LIB_NAME` | Library name e.g. `clang_rt.profile-x86_64`, or the string `"@native"` |
///
/// If the crate to build is in the current workspace, several flags will be added to the command line:
///
/// | Flag | Reason |
/// |------|--------|
/// | `-Zprofile` (nightly rustc) | Enable coverage |
/// | `-Cpasses=insert-gcov-profiling` (stable rustc) | Enable coverage |
/// | `-Clink-dead-code` | Do compile functions not called by anyone, so the function can appear red in the report |
/// | `-Coverflow-checks=off` | Disable overflow checks, which create unnecessary branches. |
/// | `-Cinline-threshold=0` | Disable inlining, which complicates control flow. |
/// | `-Ccodegen-units=1` | Disable ThinLTO which corrupts debuginfo (see [rustc issue #45511]). |
///
/// Additionally, all GCNO files generated will be moved to `$COV_BUILD_PATH/gcno/` after the build succeeds.
///
/// # Panics
///
/// Panics when any of the above environment variables is not set.
///
/// [rustc issue #45511]: https://github.com/rust-lang/rust/issues/45511
pub fn rustc<'a, I: Iterator<Item = &'a OsStr> + Clone>(args: I) -> Result<()> {
    let rustc_path = env::var_os("COV_RUSTC").expect("COV_RUSTC");
    let cov_build_path_os = env::var_os("COV_BUILD_PATH").expect("COV_BUILD_PATH");
    let cov_build_path = Path::new(&cov_build_path_os);
    let workspace_path = parent_3(cov_build_path);
    let is_local = is_rustc_compiling_local_crate(args.clone(), workspace_path);

    let mut cmd = Command::new(rustc_path);
    cmd.args(args);

    if is_local {
        let profiler_lib_path = env::var_os("COV_PROFILER_LIB_PATH").expect("COV_PROFILER_LIB_PATH");
        let profiler_lib_name = env::var_os("COV_PROFILER_LIB_NAME").expect("COV_PROFILER_LIB_NAME");
        debug!("Profiler: -L {:?} -l {:?}", profiler_lib_path, profiler_lib_name);
        if profiler_lib_path == OsStr::new("@native") && profiler_lib_name == OsStr::new("@native") {
            cmd.arg("-Zprofile");
        } else {
            cmd.arg("-Cpasses=insert-gcov-profiling").arg("-L").arg(profiler_lib_path).arg("-l").arg(profiler_lib_name);
        }
        cmd.args(&[
            "-Clink-dead-code",
            "-Coverflow-checks=off",
            "-Cinline-threshold=0",
            "-Ccodegen-units=1",
            // "-Zdebug-macros", // don't enable, makes the gcno graph involving `assert!` even worse.
        ]);
    }

    debug!("Executing {:?}", cmd);

    cmd.ensure_success("rustc")?;
    if is_local {
        move_gcov_files(cov_build_path, OsStr::new("gcno"))?;
    }

    Ok(())
}

/// Runs doc-test by forwarding `args` to `rustdoc`.
///
/// This function requires several environment variables to be set, otherwise it will panic.
///
/// | Environment variable | Meaning |
/// |----------------------|---------|
/// | `COV_RUSTDOC` | Path to `rustdoc` executable |
/// | `COV_BUILD_PATH` | Path to `target/cov/build/` of the workspace |
/// | `COV_PROFILER_LIB_PATH` | Path to folder containing `libclang_rt.profile*.a`, or the string `"@native"` |
///
/// All GCDA files generated will be moved to `$COV_BUILD_PATH/gcda/` after the test succeeds.
///
/// # Panics
///
/// Panics when any of the above environment variables is not set.
pub fn rustdoc<'a, I: Iterator<Item = &'a OsStr>>(args: I) -> Result<()> {
    let rustdoc_path = env::var_os("COV_RUSTDOC").expect("COV_RUSTDOC");
    let cov_build_path_os = env::var_os("COV_BUILD_PATH").expect("COV_BUILD_PATH");
    let cov_build_path = Path::new(&cov_build_path_os);

    let mut cmd = Command::new(rustdoc_path);

    let link_dir = env::var_os("COV_PROFILER_LIB_PATH").expect("COV_PROFILER_LIB_PATH");
    if link_dir != OsStr::new("@native") {
        cmd.arg("-L").arg(link_dir);
    }

    cmd.args(args);
    debug!("Executing {:?}", cmd);

    cmd.ensure_success("rustdoc")?;
    move_gcov_files(cov_build_path, OsStr::new("gcda"))?;

    Ok(())
}

/// Executes a program. The first string from `args` will be the path of the program to execute, and the rest will be
/// command line arguments sent to that program.
///
/// This function requires several environment variables to be set, otherwise it will panic.
///
/// | Environment variable | Meaning |
/// |----------------------|---------|
/// | `COV_BUILD_PATH` | Path to `target/cov/build/` of the workspace |
///
/// All GCDA files generated will be moved to `$COV_BUILD_PATH/gcda/` after the program succeeds.
///
/// # Panics
///
/// Panics when any of the above environment variables is not set.
pub fn run<'a, I: Iterator<Item = &'a OsStr>>(mut args: I) -> Result<()> {
    let cov_build_path_os = env::var_os("COV_BUILD_PATH").expect("COV_BUILD_PATH");
    let cov_build_path = Path::new(&cov_build_path_os);

    let mut cmd = Command::new(args.next().expect("launcher"));
    cmd.args(args);
    debug!("Executing {:?}", cmd);

    cmd.ensure_success("test")?;
    move_gcov_files(cov_build_path, OsStr::new("gcda"))?;

    Ok(())
}

/// Moves all files with the given `extension` to `[cov_build_path]/[extension]/`, and renames them uniquely so that
/// there won't be file name collision inside that folder.
///
/// # Examples
///
/// ```no_run
/// use std::path::Path;
/// use cargo_cov::shim::move_gcov_files;
///
/// # fn main() { run().unwrap(); }
/// # fn run() -> ::std::io::Result<()> {
/// let build_folder = Path::new("workspace/target/cov/build");
/// move_gcov_files(build_folder, OsStr::new("gcda"))?;
/// // All `*.gcda` files found inside `workspace/target/cov/build` will now be moved to
/// // `workspace/target/cov/build/gcda`.
/// # }
/// ```
pub fn move_gcov_files(cov_build_path: &Path, extension: &OsStr) -> Result<()> {
    let mut rng = thread_rng();
    let mut dest_path = join_2(cov_build_path, extension, "*");

    let mut lock_file = LockFile::new(cov_build_path)?;

    let it = WalkDir::new(cov_build_path).into_iter().filter_entry(|entry| {
        let file_type = entry.file_type();
        let path = entry.path();
        if file_type.is_dir() && entry.depth() == 1 {
            let file_name = path.file_name();
            if file_name == Some(OsStr::new("gcda")) || file_name == Some(OsStr::new("gcno")) {
                return false;
            }
        } else if file_type.is_file() && path.extension() != Some(extension) {
            return false;
        }
        true
    });

    for entry in it {
        let entry = entry?;
        if !entry.file_type().is_file() {
            continue;
        }

        let source_path = entry.path();

        loop {
            let mut filename = OsString::from(format!("{:016x}.", rng.gen::<u64>()));
            filename.push(source_path.file_stem().unwrap_or_else(|| OsStr::new("?")));
            filename.push(OsStr::new("."));
            filename.push(extension);
            dest_path.set_file_name(filename);
            if !dest_path.exists() {
                break;
            }
        }

        trace!("mv {:?} {:?}", source_path, dest_path);
        rename(source_path, &dest_path).chain_err(|| format!("cannot move `{}` to `{}`", source_path.display(), dest_path.display()))?;
    }

    lock_file.unlock()
}

struct LockFile(Option<File>);

impl LockFile {
    /// Tries to obtain a file lock, which prevents multiple processes from doing to [`move_gcov_files`] action at the
    /// same time.
    ///
    /// [`move_gcov_files`]: ./fn.move_gcov_files.html
    fn new(cov_build_path: &Path) -> Result<LockFile> {
        let lock_file = File::open(cov_build_path.join("rustc-shim.bat"))?;
        lock_file.lock_exclusive()?;
        Ok(LockFile(Some(lock_file)))
    }

    /// Unlocks the file immediately.
    fn unlock(&mut self) -> Result<()> {
        if let Some(lock_file) = self.0.take() {
            lock_file.unlock()?;
        }
        Ok(())
    }
}

impl Drop for LockFile {
    /// Unlocks the file, but ignore errors.
    fn drop(&mut self) {
        let _ = self.unlock();
    }
}
