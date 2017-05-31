extern crate rand;

use rand::{Rng, thread_rng};

use std::env;
use std::ffi::OsStr;
use std::fs::{read_dir, rename};
use std::io::Result;
use std::path::PathBuf;
use std::process::{Command, exit};

fn main() {
    let mut args = env::args_os();
    args.next().unwrap();
    let first_arg = args.next().unwrap();

    let mut cmd = if first_arg == OsStr::new("--test") {
        let rustdoc = env::var_os("COV_RUSTDOC").unwrap();
        let link_dir = env::var_os("COV_RUSTDOC_PROFILER_LIB_PATH").unwrap();
        let mut cmd = Command::new(rustdoc);
        cmd.args(&["--test", "-L"]).arg(link_dir);
        cmd
    } else {
        Command::new(first_arg)
    };
    cmd.args(args);

    let exit_status = cmd.status().expect("launch");
    move_gcda().expect("move *.gcda");
    exit(exit_status.code().unwrap_or(101));
}

fn move_gcda() -> Result<()> {
    let mut rng = thread_rng();

    let src_folder: PathBuf = env::var_os("COV_BUILD_PATH").unwrap().into();
    let mut dest_path = src_folder.join("gcda");
    dest_path.push("_.gcda");

    for entry in read_dir(src_folder)? {
        let path = entry?.path();
        if path.extension() == Some(OsStr::new("gcda")) {
            loop {
                dest_path.set_file_name(format!("{:016x}.gcda", rng.gen::<u64>()));
                if !dest_path.exists() {
                    break;
                }
            }
            rename(path, &dest_path)?;
        }
    }

    Ok(())
}
