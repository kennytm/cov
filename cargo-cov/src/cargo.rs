use argparse::SpecialMap;
use error::{ErrorKind, Result};
use lookup::*;
use utils::{CommandExt, OptionExt, clean_dir, set_executable};

use cov::IntoStringLossy;
use serde_json::from_reader;
use shell_escape::escape;

use std::borrow::Cow;
use std::collections::HashMap;
use std::env::current_exe;
use std::ffi::{OsStr, OsString};
use std::fs::{File, canonicalize, create_dir_all};
use std::io::Write;
use std::iter::once;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

include!(concat!(env!("OUT_DIR"), "/host.rs"));

#[derive(Debug)]
pub struct Cargo<'a> {
    cargo_path: OsString,
    rustc_path: String,
    rustdoc_path: String,
    manifest_path: PathBuf,
    cov_build_path: PathBuf,
    target: &'a str,
    profiler_lib_path: Cow<'static, str>,
    profiler_lib_name: Cow<'a, str>,
    forward_args: Vec<&'a OsStr>,
}

impl<'a> Cargo<'a> {
    pub fn new(special_args: SpecialMap<'a>, forward_args: Vec<&'a OsStr>) -> Result<Cargo<'a>> {
        let cargo_path = find_cargo();
        let rustc_path = find_rustc("RUSTC");
        let rustdoc_path = find_rustc("RUSTDOC");

        let manifest_path = match special_args.get("manifest-path") {
            Some(p) => canonicalize(p)?,
            None => locate_project(&cargo_path)?,
        };

        let metadata = parse_metadata(&cargo_path, &manifest_path)?;
        let mut cov_build_path = metadata.target_directory.unwrap_or_catch(|| find_target_path(&manifest_path))?;
        cov_build_path.push("cov");
        cov_build_path.push("build");
        create_dir_all(&cov_build_path)?;

        let target = special_args.get("target").and_then(|s| s.to_str()).unwrap_or(HOST);
        let (profiler_lib_path, profiler_lib_name) = match special_args.get("profiler") {
            Some(&path) => {
                let (p, n) = split_profiler_lib(Path::new(path))?;
                (Cow::Owned(canonicalize(p)?.into_string_lossy()), Cow::Borrowed(n))
            },
            None => {
                if supports_built_in_profiler(&rustc_path, &cov_build_path, target) {
                    (Cow::Borrowed("@native"), Cow::Borrowed("@native"))
                } else {
                    let (p, n) = find_native_profiler_lib(target)?;
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
        })
    }

    pub fn cov_build_path(&self) -> &Path {
        &self.cov_build_path
    }

    /// Prepares the coverage folder for building.
    fn prepare_cov_build_path(&self) -> Result<()> {
        let self_path = match current_exe() {
            Ok(path) => escape(Cow::Owned(path.into_string_lossy())).into_owned(),
            Err(_) => "cargo".to_owned(),
        };

        create_dir_all(self.cov_build_path.join("gcno"))?;
        create_dir_all(self.cov_build_path.join("gcda"))?;

        let rustc_shim = self.write_shim(&self_path, "rustc-shim.bat")?;
        let rustdoc_shim = self.write_shim(&self_path, "rustdoc-shim.bat")?;
        let test_runner = self.write_shim(&self_path, "test-runner.bat")?;

        let config_bytes = ::toml::to_vec(&CargoConfig {
            build: CargoConfigBuild {
                target_dir: ".",
                rustc: &rustc_shim,
                rustdoc: &rustdoc_shim,
            },
            target: once((self.target, CargoConfigTarget { runner: &[&test_runner] })).collect(),
        })?;

        let mut config_path = self.cov_build_path.join(".cargo");
        create_dir_all(&config_path)?;
        config_path.push("config");
        let mut config = File::create(config_path)?;
        config.write_all(&config_bytes)?;

        Ok(())
    }

    fn write_shim(&self, self_path: &str, shim_name: &str) -> Result<PathBuf> {
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

impl<'a> Cargo<'a> {
    pub fn forward(self, subcommand: &str) -> Result<()> {
        self.prepare_cov_build_path()?;
        let mut cmd = Command::new(self.cargo_path);
        cmd.current_dir(&self.cov_build_path)
            .env("COV_RUSTC", self.rustc_path)
            .env("COV_RUSTDOC", self.rustdoc_path)
            .env("COV_BUILD_PATH", self.cov_build_path)
            .env("COV_PROFILER_LIB_PATH", &*self.profiler_lib_path)
            .env("COV_PROFILER_LIB_NAME", &*self.profiler_lib_name)
            .arg(subcommand)
            .args(&["--target", self.target, "--manifest-path"])
            .arg(self.manifest_path)
            .args(self.forward_args);

        progress!("Delegate", "{:?}", cmd);

        cmd.ensure_success("cargo")
    }

    pub fn clean(&self, gcda_only: bool, report: bool) -> Result<()> {
        fn do_clean(folder: &Path) -> Result<()> {
            progress!("Remove", "{}", folder.display());
            clean_dir(folder)?;
            Ok(())
        }

        if gcda_only {
            do_clean(&self.cov_build_path.join("gcda"))?;
        } else {
            do_clean(&self.cov_build_path)?;
        }
        if report {
            do_clean(&self.cov_build_path.with_file_name("report"))?;
        }
        Ok(())
    }
}


fn locate_project(cargo_path: &OsStr) -> Result<PathBuf> {
    let child = Command::new(cargo_path) // @rustfmt-force-break
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::inherit())
        .arg("locate-project")
        .spawn()?;
    let project_location: ProjectLocation = from_reader(child.stdout.unwrap())?;
    Ok(project_location.root)
}

#[derive(Debug, Deserialize)]
struct ProjectLocation {
    root: PathBuf,
}

#[derive(Debug, Deserialize)]
struct Metadata {
    target_directory: Option<PathBuf>,
}

fn parse_metadata(cargo_path: &OsStr, manifest_path: &Path) -> Result<Metadata> {
    let child = Command::new(cargo_path) // @rustfmt-force-break
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::inherit())
        .args(&["metadata", "--no-deps", "--format-version", "1", "--manifest-path"])
        .arg(manifest_path)
        .spawn()?;
    let metadata = from_reader(child.stdout.unwrap())?;
    Ok(metadata)
}

fn find_target_path(manifest_path: &Path) -> Result<PathBuf> {
    let mut base = manifest_path.to_owned();
    while base.pop() {
        base.push("target");
        if base.is_dir() {
            return Ok(base);
        }
        base.set_file_name("Cargo.lock");
        let has_cargo_lock = base.is_file();
        if has_cargo_lock {
            base.set_file_name("target");
            return Ok(base);
        }
        base.pop();
    }
    Err(ErrorKind::TargetDirectoryNotFound.into())
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
}

#[derive(Debug, Serialize)]
struct CargoConfigTarget<'a> {
    runner: &'a [&'a Path],
}

fn supports_built_in_profiler(rustc: &str, temp_path: &Path, target: &str) -> bool {
    let result = Command::new(rustc)
        .stdin(Stdio::null())
        .stdout(Stdio::inherit())
        .stderr(Stdio::inherit())
        .args(
            &[
                "-",
                "-Zprofile",
                "--crate-name",
                "___",
                "--crate-type",
                "lib",
                "--target",
                target,
                "--out-dir",
            ],
        )
        .arg(temp_path)
        .status()
        .map(|s| s.success())
        .unwrap_or(false);
    debug!("supports_built_in_profiler({:?}, {:?}, {:?}) = {}", rustc, temp_path, target, result);
    result
}
