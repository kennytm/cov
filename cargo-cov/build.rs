use std::env;
use std::fs::File;
use std::io::Write;
use std::path::PathBuf;

fn main() {
    let target = env::var("TARGET").unwrap();
    let mut out_path: PathBuf = env::var_os("OUT_DIR").unwrap().into();
    out_path.push("host.rs");
    let mut file = File::create(out_path).unwrap();
    write!(file, r#"const HOST: &str = "{}";"#, target).unwrap();

    println!("cargo:rerun-if-changed=build.rs");
}
