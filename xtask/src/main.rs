//! Dev-only build automation for `media-rs`.
//!
//! Subcommands:
//!   generate [-o FILE] [--version V]
//!       Generate bindings for the current host from `wrapper.h`.
//!       Default output: `bindings-<os>.rs`.
//!
//!   merge OS=FILE [OS=FILE ...] -o FILE
//!       Union per-OS bindgen outputs into one committed bindings file. `OS` is the Rust
//!       `target_os` value, e.g. `macos`, `linux`, `windows`.
//!
//! Example CI usage:
//!   cargo run -p xtask -- generate -o bindings-macos.rs
//!   cargo run -p xtask -- merge macos=bindings-macos.rs linux=bindings-linux.rs \
//!       windows=bindings-windows.rs -o src/sys/bindings.rs

mod binaries;
mod generate;
mod merge;

use std::path::PathBuf;

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    match args.first().map(String::as_str) {
        Some("generate") => cmd_generate(&args[1..]),
        Some("merge") => cmd_merge(&args[1..]),
        other => {
            eprintln!("xtask: unknown command {other:?}");
            eprintln!("usage: xtask <generate|merge> ...");
            std::process::exit(2);
        }
    }
}

fn cmd_generate(args: &[String]) {
    let mut out: Option<PathBuf> = None;
    let mut version = binaries::VERSION.to_string();

    let mut i = 0;
    while i < args.len() {
        match args[i].as_str() {
            "-o" | "--out" => {
                out = Some(PathBuf::from(expect_value(args, &mut i, "-o")));
            }
            "--version" => {
                version = expect_value(args, &mut i, "--version");
            }
            other => panic!("xtask generate: unexpected argument `{other}`"),
        }
        i += 1;
    }

    generate::run(out, &version);
}

fn cmd_merge(args: &[String]) {
    let mut inputs: Vec<(String, PathBuf)> = Vec::new();
    let mut out: Option<PathBuf> = None;

    let mut i = 0;
    while i < args.len() {
        match args[i].as_str() {
            "-o" | "--out" => {
                out = Some(PathBuf::from(expect_value(args, &mut i, "-o")));
            }
            spec => {
                let (os, path) = spec
                    .split_once('=')
                    .unwrap_or_else(|| panic!("xtask merge: expected OS=FILE, got `{spec}`"));
                inputs.push((os.to_string(), PathBuf::from(path)));
            }
        }
        i += 1;
    }

    let out = out.expect("xtask merge: missing -o OUTPUT");
    merge::run(&inputs, &out);
}

fn expect_value(args: &[String], i: &mut usize, flag: &str) -> String {
    *i += 1;
    args.get(*i)
        .unwrap_or_else(|| panic!("xtask: {flag} requires a value"))
        .clone()
}
