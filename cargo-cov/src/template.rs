#![cfg_attr(feature="cargo-clippy", allow(needless_pass_by_value))]
// The pass-by-value is mandated by Tera.

use sourcepath::{SOURCE_TYPE_MACROS, identify_source_path};
use utils::ValueExt;

use md5;
use rustc_demangle::demangle;
use serde_json::Value;
use tera::{Result, Tera};

use std::collections::HashMap;
use std::path::MAIN_SEPARATOR;

pub fn new(dirs: &str) -> Result<Tera> {
    let mut tera = Tera::new(dirs)?;
    tera.autoescape_on(Vec::new());
    tera.register_filter("md5", compute_md5);
    tera.register_filter("clamp", clamp);
    tera.register_filter("to_fixed", to_fixed);
    tera.register_filter("filename", filename);
    tera.register_filter("simplify_source_path", simplify_source_path);
    tera.register_filter("coalesce", coalesce);
    tera.register_filter("demangle", demangle_rust);
    tera.register_global_function("debug_it", Box::new(debug_it));
    Ok(tera)
}


fn compute_md5(value: Value, _: HashMap<String, Value>) -> Result<Value> {
    let string = value.as_str().ok_or("expecting string to compute md5")?;
    let res = format!("{:x}", md5::compute(string));
    Ok(Value::String(res))
}

fn clamp(value: Value, options: HashMap<String, Value>) -> Result<Value> {
    let number = value.as_f64().ok_or("expecting number to clamp")?;
    let min = options.get("min").and_then(Value::as_f64).ok_or("clamp should have a min number")?;
    let max = options.get("max").and_then(Value::as_f64).ok_or("clamp should have a max number")?;
    Ok(number.max(min).min(max).into())
}

fn to_fixed(value: Value, options: HashMap<String, Value>) -> Result<Value> {
    let number = value.as_f64().ok_or("expecting number to format")?;
    let digits = options.get("precision").and_then(Value::as_u64).unwrap_or(0) as usize;
    Ok(Value::String(format!("{:.*}", digits, number)))
}

fn filename(value: Value, _: HashMap<String, Value>) -> Result<Value> {
    let path = value.as_str().ok_or("expecting path")?;
    let start = path.rfind(MAIN_SEPARATOR).map_or(0, |s| s + MAIN_SEPARATOR.len_utf8());
    Ok(Value::from(&path[start..]))
}

fn simplify_source_path(value: Value, mut options: HashMap<String, Value>) -> Result<Value> {
    let path = value.try_into_string().ok_or("expecting source path")?;
    let mut crate_path = options.remove("crate_path").and_then(Value::try_into_string).ok_or("simplify_source_path should provide the crate_path")?;
    crate_path.push(MAIN_SEPARATOR);

    let (source_type, stripped_len) = identify_source_path(&path, &crate_path);
    let simplified = if source_type == SOURCE_TYPE_MACROS {
        path
    } else {
        format!("{}{}{}", source_type.prefix(), MAIN_SEPARATOR, &path[stripped_len..])
    };

    Ok(Value::String(simplified))
}

fn coalesce(value: Value, mut options: HashMap<String, Value>) -> Result<Value> {
    if value == Value::Null {
        options.remove("default").ok_or_else(|| "coalesce should provide default value".into())
    } else {
        Ok(value)
    }
}

fn demangle_rust(value: Value, _: HashMap<String, Value>) -> Result<Value> {
    let name = value.as_str().ok_or("expecting string to demangle")?;
    Ok(Value::String(demangle(name).to_string()))
}

fn debug_it(args: HashMap<String, Value>) -> Result<Value> {
    debug!("DEBUG FROM TEMPLATE: {:#?}", args);
    Ok(Value::Null)
}
