//! Build environment information for `cargo cov`.

use argparse::SpecialMap;
use error::{ErrorKind, Result, ResultExt};
use lookup::*;
use shim::move_gcov_files;
use utils::{CommandExt, clean_dir, set_executable};

use cov::IntoStringLossy;
use serde_json::from_reader;
use shell_escape::escape;
use tempfile::TempDir;

use std::borrow::Cow;
use std::collections::HashMap;
use std::env::current_exe;
use std::ffi::{OsStr, OsString};
use std::fs::{File, canonicalize, create_dir, create_dir_all};
use std::io::{self, Write};
use std::iter::once;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

include!(concat!(env!("OUT_DIR"), "/host.rs"));

/// Container to store information of the host environment and important command line arguments.
#[derive(Debug)]
pub struct Cargo<'a> {
    /// Path to `cargo`.
    cargo_path: OsString,
    /// Path to `rustc`.
    rustc_path: String,
    /// Path to `rustdoc`.
    rustdoc_path: String,
    /// Canonical path to `Cargo.toml`.
    manifest_path: PathBuf,
    /// Canonical path to `target/cov/build`.
    cov_build_path: PathBuf,
    /// Build target triples.
    target: &'a str,
    /// Canonical path to the folder containing the compiler-rt profiler library, or the string `"@native"` if building
    /// for nightly Rust.
    profiler_lib_path: Cow<'static, str>,
    /// Name of the compiler-rt profiler library, or the string `"@native"` if building for nightly Rust.
    profiler_lib_name: Cow<'a, str>,
    /// Arguments to be forwarded to `cargo`.
    forward_args: Vec<&'a OsStr>,
    /// List of packages in this workspace.
    workspace_packages: Vec<String>,
}

impl<'a> Cargo<'a> {
    /// Creates a build environment from the command line arguments.
    ///
    /// `special_args` and `forward_args` should be parsed from [`argparse::normalize()`].
    ///
    /// [`argparse::normalize()`]: ../argparse/fn.normalize.html
    pub fn new(special_args: SpecialMap<'a>, forward_args: Vec<&'a OsStr>) -> Result<Cargo<'a>> {
        let cargo_path = find_cargo();
        let rustc_path = find_rustc("RUSTC");
        let rustdoc_path = find_rustc("RUSTDOC");

        let manifest_path = match special_args.get("manifest-path") {
            Some(p) => canonicalize(p)?,
            None => locate_project(&cargo_path).chain_err(|| "Cargo.toml not found")?,
        };

        let metadata = parse_metadata(&cargo_path, &manifest_path).chain_err(|| "Cannot parse workspace metadata")?;
        let mut cov_build_path = metadata.target_directory.or_else(|| find_target_path(&manifest_path)).ok_or(ErrorKind::TargetDirectoryNotFound)?;
        cov_build_path.push("cov");
        cov_build_path.push("build");
        create_dir_all(&cov_build_path).chain_err(|| "Cannot prepare coverage build directory")?;

        let mut workspace_packages = metadata.workspace_members;
        for pkg_id in &mut workspace_packages {
            let space_index = pkg_id.find(' ').unwrap_or_else(|| pkg_id.len());
            pkg_id.truncate(space_index);
        }

        let target = special_args.get("target").and_then(|s| s.to_str()).unwrap_or(HOST);
        let (profiler_lib_path, profiler_lib_name) = match special_args.get("profiler") {
            Some(&path) => {
                let (p, n) = split_profiler_lib(Path::new(path)).chain_err(|| "Cannot parse user-provided profiler library")?;
                (Cow::Owned(canonicalize(p)?.into_string_lossy()), Cow::Borrowed(n))
            },
            None => {
                if supports_built_in_profiler(&rustc_path, target) {
                    (Cow::Borrowed("@native"), Cow::Borrowed("@native"))
                } else {
                    let (p, n) = find_native_profiler_lib(target).chain_err(|| "Native profiler library not found")?;
                    (Cow::Owned(canonicalize(p)?.into_string_lossy()), Cow::Owned(n))
                }
            },
        };
        debug!("Profiler: -L {} -l {}", profiler_lib_path, profiler_lib_name);

        Ok(Cargo {
            cargo_path,
            rustc_path,
            rustdoc_path,
            manifest_path,
            cov_build_path,
            target,
            profiler_lib_path,
            profiler_lib_name,
            forward_args,
            workspace_packages,
        })
    }

    /// Obtains the `target/cov/build` path and transfers ownership.
    pub fn into_cov_build_path(self) -> PathBuf {
        self.cov_build_path
    }

    /// Prepares the coverage folder for building.
    ///
    /// This method will write a `.cargo/config` file which:
    ///
    /// * Place all output artifact to `target/cov/build/` instead of `target/`, so the profiled objects will not
    ///   interfere with normal objects
    /// * Configure `cargo` to use [`cargo-cov` shims](../shim/index.html) when building and running tests.
    /// * Disable incremental compilation to workaround rustc issue [#50203].
    ///
    /// [#50203]: https://github.com/rust-lang/rust/issues/50203
    fn prepare_cov_build_path(&self) -> Result<()> {
        let self_path = match current_exe() {
            Ok(path) => escape(Cow::Owned(path.into_string_lossy())).into_owned(),
            Err(_) => "cargo-cov".to_owned(),
        };

        create_dir_all(self.cov_build_path.join("gcno"))?;
        create_dir_all(self.cov_build_path.join("gcda"))?;

        let rustc_shim = self.write_shim(&self_path, "rustc-shim.bat")?;
        let rustdoc_shim = self.write_shim(&self_path, "rustdoc-shim.bat")?;
        let test_runner = self.write_shim(&self_path, "test-runner.bat")?;
        let test_runner_slice: &[&Path] = &[&test_runner];

        let target = if let Ok(true) = supports_target_runner(&self.cargo_path) {
            once((
                self.target,
                CargoConfigTarget {
                    runner: test_runner_slice,
                },
            )).collect()
        } else {
            HashMap::new()
        };

        let config_bytes = ::toml::to_vec(&CargoConfig {
            build: CargoConfigBuild {
                target_dir: ".",
                rustc: &rustc_shim,
                rustdoc: &rustdoc_shim,
                incremental: false,
            },
            target,
        })?;

        let mut config_path = self.cov_build_path.join(".cargo");
        create_dir_all(&config_path)?;
        config_path.push("config");
        let mut config = File::create(config_path)?;
        config.write_all(&config_bytes)?;

        Ok(())
    }

    /// Writes the content of a shim script.
    fn write_shim(&self, self_path: &str, shim_name: &str) -> io::Result<PathBuf> {
        #[cfg(unix)]
        const HEADER: &str = "#!/bin/sh";
        #[cfg(unix)]
        const FORWARD_ARGS: &str = "\"$@\"";
        #[cfg(windows)]
        const HEADER: &str = "@echo off";
        #[cfg(windows)]
        const FORWARD_ARGS: &str = "%*";

        let shim_path = self.cov_build_path.join(shim_name);
        let mut file = File::create(&shim_path)?;
        set_executable(&file)?;
        write!(file, "{}\n{} {} {}", HEADER, self_path, shim_name, FORWARD_ARGS)?;
        Ok(shim_path)
    }
}

bitflags! {
    /// Collection of things to be cleaned.
    ///
    /// These bitflags would be used in [`Cargo::clean()`].
    ///
    /// [`Cargo::clean()`]: ./struct.Cargo.html#method.clean
    pub struct CleanTargets: u8 {
        /// Delete the `target/cov/build/gcda/` folder.
        const BUILD_GCDA = 1;
        /// Delete the `target/cov/build/gcno/` folder and built artifacts of all crates in the current workspace.
        const BUILD_GCNO = 2;
        /// Delete the whole `target/cov/build/` folder.
        const BUILD_EXTERNAL = 4;
        /// Delete the `target/cov/report` folder.
        const REPORT = 8;
    }
}

impl<'a> Cargo<'a> {
    /// Runs the real cargo subcommand (build, test, run).
    pub fn forward(self, subcommand: &str) -> Result<()> {
        self.prepare_cov_build_path()?;
        let mut cmd = Command::new(self.cargo_path);
        cmd.current_dir(&self.cov_build_path)
            .env("COV_RUSTC", self.rustc_path)
            .env("COV_RUSTDOC", self.rustdoc_path)
            .env("COV_BUILD_PATH", &self.cov_build_path)
            .env("COV_PROFILER_LIB_PATH", &*self.profiler_lib_path)
            .env("COV_PROFILER_LIB_NAME", &*self.profiler_lib_name)
            .arg(subcommand)
            .arg("--manifest-path")
            .arg(self.manifest_path);
        if self.target != HOST {
            cmd.args(&["--target", self.target]);
        }
        cmd.args(self.forward_args);

        progress!("Delegate", "{:?}", cmd);

        cmd.ensure_success("cargo")?;
        if subcommand == "test" || subcommand == "run" {
            // Before 1.19, the test-runner is absent, so we need to move them outside of the shim.
            move_gcov_files(&self.cov_build_path, OsStr::new("gcda"))?;
        }

        Ok(())
    }

    /// Cleans the `target/cov` directory.
    pub fn clean(&self, clean_targets: CleanTargets) -> Result<()> {
        fn do_clean(folder: &Path) -> Result<()> {
            progress!("Remove", "{}", folder.display());
            clean_dir(folder)?;
            Ok(())
        }

        if clean_targets.contains(CleanTargets::BUILD_EXTERNAL) {
            do_clean(&self.cov_build_path)?;
        } else {
            if clean_targets.contains(CleanTargets::BUILD_GCDA) {
                do_clean(&self.cov_build_path.join("gcda"))?
            }
            if clean_targets.contains(CleanTargets::BUILD_GCNO) {
                let mut cmd = Command::new(&self.cargo_path);
                cmd.current_dir(&self.cov_build_path)
                    .env("RUSTC", &self.rustc_path) // No need to run our shim.
                    .args(&["clean", "--manifest-path"])
                    .arg(&self.manifest_path);
                if self.target != HOST {
                    cmd.args(&["--target", self.target]);
                }
                for pkg_name in &self.workspace_packages {
                    cmd.args(&["-p", pkg_name]);
                }
                progress!("Delegate", "{:?}", cmd);
                cmd.ensure_success("cargo")?;
                do_clean(&self.cov_build_path.join("gcno"))?
            }
        }

        if clean_targets.contains(CleanTargets::REPORT) {
            do_clean(&self.cov_build_path.with_file_name("report"))?;
        }
        Ok(())
    }
}

/// Locates the path to `Cargo.toml` if it is not specified in the command line.
fn locate_project(cargo_path: &OsStr) -> Result<PathBuf> {
    let child = Command::new(cargo_path) // @rustfmt-force-break
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .arg("locate-project")
        .spawn()?;
    let project_location: ProjectLocation = from_reader(child.stdout.expect("stdout"))?;
    Ok(project_location.root)
}

#[derive(Debug, Deserialize)]
struct ProjectLocation {
    root: PathBuf,
}

#[derive(Debug, Deserialize)]
struct Metadata {
    workspace_members: Vec<String>,
    target_directory: Option<PathBuf>,
}

/// Obtains the `target/` directory for a crate using `cargo metadata`.
///
/// This method is supported only starting from Rust 1.19.
fn parse_metadata(cargo_path: &OsStr, manifest_path: &Path) -> io::Result<Metadata> {
    let child = Command::new(cargo_path) // @rustfmt-force-break
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::inherit())
        .args(&["metadata", "--no-deps", "--format-version", "1", "--manifest-path"])
        .arg(manifest_path)
        .spawn()?;
    let metadata = from_reader(child.stdout.expect("stdout"))?;
    Ok(metadata)
}

/// Obtains the `target/` path by searching for `Cargo.lock` or the `target/` directory itself in the ancestors.
fn find_target_path(manifest_path: &Path) -> Option<PathBuf> {
    let mut base = manifest_path.to_owned();
    while base.pop() {
        base.push("target");
        if base.is_dir() {
            return Some(base);
        }
        base.set_file_name("Cargo.lock");
        let has_cargo_lock = base.is_file();
        if has_cargo_lock {
            base.set_file_name("target");
            return Some(base);
        }
        base.pop();
    }
    None
}

#[derive(Debug, Serialize)]
struct CargoConfig<'a> {
    target: HashMap<&'a str, CargoConfigTarget<'a>>,
    build: CargoConfigBuild<'a>,
}

#[derive(Debug, Serialize)]
struct CargoConfigBuild<'a> {
    #[serde(rename = "target-dir")]
    target_dir: &'a str,
    rustc: &'a Path,
    rustdoc: &'a Path,
    incremental: bool,
}

#[derive(Debug, Serialize)]
struct CargoConfigTarget<'a> {
    runner: &'a [&'a Path],
}

/// Checks whether the `rustc` compiling for the specific target supports the `-Zprofile` flag.
///
/// `-Zprofile` is only supported on nightly Rust since 1.19, for a selected list of targets.
fn supports_built_in_profiler(rustc: &str, target: &str) -> bool {
    let dir = TempDir::new().expect("created temporary directory");

    let result = Command::new(rustc)
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .args(&[
            "-",
            "-Zprofile",
            "--crate-name",
            "___",
            "--crate-type",
            "lib",
            "--target",
            target,
            "--out-dir",
        ])
        .arg(dir.path())
        .status()
        .map(|s| s.success())
        .unwrap_or(false);
    debug!("supports_built_in_profiler({:?}, {:?}) = {}", rustc, target, result);
    result
}

/// Checks whether the `cargo` supports the target-runner configuration.
///
/// Target-runner is only supported since Rust 1.19.
fn supports_target_runner(cargo: &OsStr) -> Result<bool> {
    use std::io::Write;

    let dir = TempDir::new()?;

    let mut cargo_config_path = dir.path().join(".cargo");
    create_dir(&cargo_config_path)?;
    cargo_config_path.push("config");
    let mut file = File::create(cargo_config_path)?;
    write!(file, "[target.{}]\nrunner = \"echo\"", HOST)?;
    drop(file);

    let manifest_path = dir.path().join("Cargo.toml");
    let mut file = File::create(manifest_path)?;
    write!(
        file,
        r#"
            #![cfg(test)] /* Note: this file doubles as `Cargo.toml` and `lib.rs`

            [package]
            name = "check_runner"
            version = "0.0.0"

            [lib]
            path = "Cargo.toml"

            # */
        "#
    )?;
    drop(file);

    let result = Command::new(cargo) // @rustfmt-force-break
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .current_dir(dir.path())
        .arg("build")
        .status()
        .map(|s| s.success())
        .unwrap_or(false);
    debug!("supports_target_runner({:?}) = {}", cargo, result);
    Ok(result)
}
