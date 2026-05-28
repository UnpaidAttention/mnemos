//! Git custom merge driver for mnemos memory `.md` files.
//!
//! Invoked by git as: `mnemos-merge-driver %A %O %B` where %A is the current
//! (ours) version that will be overwritten with the merge result, %O the
//! ancestor, %B theirs. Exit 0 = clean merge; non-zero = conflict.
//!
//! Strategy: parse YAML frontmatter on each side; merge frontmatter field by
//! field with conservative rules; concatenate bodies separated by a marker if
//! both sides changed the body, otherwise take whichever side changed.

use anyhow::{Context, Result};
use serde_yaml::{Mapping, Value};
use std::fs;
use std::path::PathBuf;

fn split(text: &str) -> (Option<String>, String) {
    let trimmed = text.trim_start();
    if let Some(rest) = trimmed.strip_prefix("---\n") {
        if let Some(end) = rest.find("\n---") {
            let fm = rest[..end].to_string();
            let body = rest[end..]
                .strip_prefix("\n---")
                .unwrap_or("")
                .trim_start_matches('\n')
                .to_string();
            return (Some(fm), body);
        }
    }
    (None, text.to_string())
}

fn merge_yaml(a: &Value, b: &Value, base: &Value) -> Value {
    use Value::*;
    match (a, b, base) {
        (Mapping(am), Mapping(bm), Mapping(basem)) => {
            let mut out = am.clone();
            for (k, bv) in bm {
                let av = am.get(k);
                let basev = basem.get(k);
                match (av, basev) {
                    (Some(av), Some(basev)) if av == basev => {
                        // a unchanged → take b
                        out.insert(k.clone(), bv.clone());
                    }
                    (None, None) => {
                        out.insert(k.clone(), bv.clone());
                    }
                    (Some(_), _) => {
                        // both modified — field-specific merge
                        let key = k.as_str().unwrap_or("");
                        out.insert(k.clone(), merge_field(key, av.unwrap(), bv, basev));
                    }
                    _ => {
                        out.insert(k.clone(), bv.clone());
                    }
                }
            }
            Mapping(out)
        }
        _ => a.clone(),
    }
}

fn merge_field(key: &str, a: &Value, b: &Value, _base: Option<&Value>) -> Value {
    match key {
        "invalid_at" => {
            if a.is_null() {
                b.clone()
            } else {
                a.clone()
            }
        }
        "superseded_by" => {
            if a.is_null() {
                b.clone()
            } else {
                a.clone()
            }
        }
        "tags" => union_sequences(a, b),
        "strength" | "importance" => max_num(a, b),
        "valid_at" | "ingested_at" | "last_accessed" => later_string(a, b),
        _ => a.clone(),
    }
}

fn union_sequences(a: &Value, b: &Value) -> Value {
    let mut out: Vec<Value> = Vec::new();
    if let Some(s) = a.as_sequence() {
        out.extend(s.clone());
    }
    if let Some(s) = b.as_sequence() {
        for v in s {
            if !out.contains(v) {
                out.push(v.clone());
            }
        }
    }
    Value::Sequence(out)
}

fn max_num(a: &Value, b: &Value) -> Value {
    match (a.as_f64(), b.as_f64()) {
        (Some(x), Some(y)) => Value::Number(serde_yaml::Number::from(x.max(y))),
        _ => a.clone(),
    }
}

fn later_string(a: &Value, b: &Value) -> Value {
    match (a.as_str(), b.as_str()) {
        (Some(x), Some(y)) => {
            if x >= y {
                a.clone()
            } else {
                b.clone()
            }
        }
        _ => a.clone(),
    }
}

fn render(fm: &Value, body: &str) -> Result<String> {
    let fm_str = serde_yaml::to_string(fm)?;
    Ok(format!("---\n{}---\n{}", fm_str, body))
}

fn main() -> Result<()> {
    let args: Vec<String> = std::env::args().skip(1).collect();
    if args.len() < 3 {
        eprintln!("usage: mnemos-merge-driver %A %O %B");
        std::process::exit(2);
    }
    let a_path = PathBuf::from(&args[0]);
    let o_path = PathBuf::from(&args[1]);
    let b_path = PathBuf::from(&args[2]);
    let a = fs::read_to_string(&a_path).context("read A")?;
    let o = fs::read_to_string(&o_path).context("read O")?;
    let b = fs::read_to_string(&b_path).context("read B")?;

    let (fa, ba) = split(&a);
    let (fo, bo) = split(&o);
    let (fb, bb) = split(&b);
    let parse = |s: &Option<String>| -> Value {
        s.as_deref()
            .map(|x| serde_yaml::from_str(x).unwrap_or(Value::Mapping(Mapping::new())))
            .unwrap_or(Value::Mapping(Mapping::new()))
    };
    let merged_fm = merge_yaml(&parse(&fa), &parse(&fb), &parse(&fo));
    let merged_body = if ba == bo {
        bb
    } else if bb == bo {
        ba
    } else {
        format!(
            "{}\n\n<!-- mnemos-merge: both sides changed body -->\n\n{}",
            ba, bb
        )
    };
    let out = render(&merged_fm, &merged_body)?;
    fs::write(&a_path, out).context("write A")?;
    Ok(())
}
