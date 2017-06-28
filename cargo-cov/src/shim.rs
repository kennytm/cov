//! Shim for rustc, rustdoc and test runner.

use argparse::is_rustc_compiling_local_crate;
use error::Result;
use utils::{CommandExt, parent_3};

use rand::{Rng, thread_rng};
use walkdir::{WalkDir, WalkDirIterator};

use std::env;
use std::ffi::{OsStr, OsString};
use std::fs::rename;
use std::path::Path;
use std::process::Command;

pub fn rustc<'a, I: Iterator<Item = &'a OsStr> + Clone>(args: I) -> Result<()> {
    let rustc_path = env::var_os("COV_RUSTC").expect("COV_RUSTC");
    let cov_build_path_os = env::var_os("COV_BUILD_PATH").expect("COV_BUILD_PATH");
    let cov_build_path = Path::new(&cov_build_path_os);
    let workspace_path = parent_3(cov_build_path);

    let mut cmd = Command::new(rustc_path);
    let is_local = is_rustc_compiling_local_crate(args.clone(), workspace_path);
    if is_local {
        let profiler_lib_path = env::var_os("COV_PROFILER_LIB_PATH").expect("COV_PROFILER_LIB_PATH");
        let profiler_lib_name = env::var_os("COV_PROFILER_LIB_NAME").expect("COV_PROFILER_LIB_NAME");
        debug!("Profiler: -L {:?} -l {:?}", profiler_lib_path, profiler_lib_name);
        if &profiler_lib_path == OsStr::new("@native") && &profiler_lib_name == OsStr::new("@native") {
            cmd.arg("-Zprofile");
        } else {
            cmd.arg("-Cpasses=insert-gcov-profiling").arg("-L").arg(profiler_lib_path).arg("-l").arg(profiler_lib_name);
        }
        cmd.args(
            &[
                "-Clink-dead-code",
                "-Coverflow-checks=off",
                "-Cinline-threshold=0",
            // "-Zdebug-macros", // don't enable, makes the gcno graph involving `assert!` even worse.
            ],
        );
    }

    cmd.args(args);
    debug!("Executing {:?}", cmd);

    cmd.ensure_success("rustc")?;
    if is_local {
        move_gcov_files(cov_build_path, OsStr::new("gcno"))?;
    }

    Ok(())
}

pub fn rustdoc<'a, I: Iterator<Item = &'a OsStr>>(args: I) -> Result<()> {
    let rustdoc_path = env::var_os("COV_RUSTDOC").expect("COV_RUSTDOC");
    let cov_build_path_os = env::var_os("COV_BUILD_PATH").expect("COV_BUILD_PATH");
    let cov_build_path = Path::new(&cov_build_path_os);

    let mut cmd = Command::new(rustdoc_path);

    let link_dir = env::var_os("COV_PROFILER_LIB_PATH").expect("COV_PROFILER_LIB_PATH");
    if &link_dir != OsStr::new("@native") {
        cmd.arg("-L").arg(link_dir);
    }

    cmd.args(args);
    debug!("Executing {:?}", cmd);

    cmd.ensure_success("rustdoc")?;
    move_gcov_files(cov_build_path, OsStr::new("gcda"))?;

    Ok(())
}

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

pub fn move_gcov_files(cov_build_path: &Path, extension: &OsStr) -> Result<()> {
    let mut rng = thread_rng();
    let mut dest_path = cov_build_path.join(extension);
    dest_path.push("*");

    let mut it = WalkDir::new(cov_build_path).into_iter().filter_entry(|entry| {
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

    while let Some(entry) = it.next() {
        let entry = entry?;
        if !entry.file_type().is_file() {
            continue;
        }

        let source_path = entry.path();

        loop {
            let mut filename = OsString::from(format!("{:016x}.", rng.gen::<u64>()));
            filename.push(extension);
            dest_path.set_file_name(filename);
            if !dest_path.exists() {
                break;
            }
        }

        trace!("mv {:?} {:?}", source_path, dest_path);
        rename(source_path, &dest_path)?;
    }

    Ok(())
}
