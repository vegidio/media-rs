//! `xtask merge` — union per-OS bindgen outputs into one committed `bindings.rs`.
//!
//! Each input is a full bindgen file for one OS (labelled with its Rust `target_os`).
//! Top-level items are keyed by `(kind, identifier)`; `extern "C"` blocks are flattened so
//! that individual foreign functions/statics dedup by name rather than by block.
//!
//!  * An item with a single distinct definition across all inputs is emitted **once,
//!    unconditionally** — so a hardware struct that only Windows could generate (e.g.
//!    `AVD3D11VADeviceContext`) is still visible when compiling on macOS. bindgen output is
//!    pure Rust, so it compiles everywhere; the matching functions only fail to *link* if
//!    actually called on the wrong OS, which the crate's future guardrails prevent.
//!  * An item whose definition genuinely differs between OSes is emitted once per variant,
//!    each `#[cfg(target_os = …)]`-gated to the OSes that produced it.

use std::collections::HashMap;
use std::path::PathBuf;

use quote::quote;
use syn::{ForeignItem, Item};

/// One distinct token form of an item, plus the set of OSes that produced it.
struct Variant {
    tokens: String,
    oses: Vec<String>,
}

struct Entry {
    is_foreign: bool,
    variants: Vec<Variant>,
}

/// `inputs` is a list of `(target_os, path)` pairs.
pub fn run(inputs: &[(String, PathBuf)], out: &PathBuf) {
    assert!(!inputs.is_empty(), "xtask merge: no input files given");

    let mut order: Vec<String> = Vec::new();
    let mut map: HashMap<String, Entry> = HashMap::new();

    // The complete set of input OSes (de-duplicated, input order preserved). A conflicting
    // item whose variants don't cover every one of these needs a fallback arm so the symbol
    // is still defined on the uncovered OSes.
    let mut all_oses: Vec<String> = Vec::new();
    for (os, _) in inputs {
        if !all_oses.iter().any(|o| o == os) {
            all_oses.push(os.clone());
        }
    }

    for (os, path) in inputs {
        let src = std::fs::read_to_string(path)
            .unwrap_or_else(|e| panic!("xtask merge: cannot read {}: {e}", path.display()));
        let file =
            syn::parse_file(&src).unwrap_or_else(|e| panic!("xtask merge: cannot parse {}: {e}", path.display()));

        for item in file.items {
            match item {
                Item::ForeignMod(fm) => {
                    for fi in fm.items {
                        let (key, tokens) = foreign_key_tokens(&fi);
                        push(&mut order, &mut map, key, tokens, os, true);
                    }
                }
                other => {
                    let (key, tokens) = item_key_tokens(&other);
                    push(&mut order, &mut map, key, tokens, os, false);
                }
            }
        }
    }

    // Assemble the merged source. Normal items keep their place; foreign items are
    // regrouped into a single unconditional `extern` block plus per-cfg blocks.
    let mut normal = String::new();
    let mut uncond_foreign = String::new();
    let mut cond_foreign = String::new();

    for key in &order {
        let entry = &map[key];
        let conflicting = entry.variants.len() > 1;
        for v in &entry.variants {
            let cfg = conflicting.then(|| cfg_attr(&v.oses));
            if entry.is_foreign {
                match &cfg {
                    Some(c) => {
                        cond_foreign.push_str(c);
                        cond_foreign.push_str("\nunsafe extern \"C\" {\n");
                        cond_foreign.push_str(&v.tokens);
                        cond_foreign.push_str("\n}\n");
                    }
                    None => {
                        uncond_foreign.push_str(&v.tokens);
                        uncond_foreign.push('\n');
                    }
                }
            } else {
                if let Some(c) = &cfg {
                    normal.push_str(c);
                    normal.push('\n');
                }
                normal.push_str(&v.tokens);
                normal.push('\n');
            }
        }

        // A conflicting *normal* item is gated per-OS; if its variants don't cover every input
        // OS, the symbol would be undefined on the uncovered ones (e.g. `AMF_RESULT` exists on
        // Linux/Windows but not macOS), breaking the unconditional items that reference it.
        // Emit a fallback arm — one variant's tokens under `#[cfg(not(any(<covered oses>)))]` —
        // so the type is *named* on every host. Foreign `fn`/`static` declarations are left
        // alone: a wrong-OS symbol only matters at link time, and the future API guards calls.
        if conflicting && !entry.is_foreign {
            let covered: Vec<String> =
                entry
                    .variants
                    .iter()
                    .flat_map(|v| v.oses.iter().cloned())
                    .fold(Vec::new(), |mut acc, o| {
                        if !acc.contains(&o) {
                            acc.push(o);
                        }
                        acc
                    });
            if all_oses.iter().any(|o| !covered.contains(o)) {
                normal.push_str(&cfg_not_any(&covered));
                normal.push('\n');
                normal.push_str(&entry.variants[0].tokens);
                normal.push('\n');
            }
        }
    }

    let mut full = String::new();
    full.push_str(&normal);
    if !uncond_foreign.is_empty() {
        full.push_str("unsafe extern \"C\" {\n");
        full.push_str(&uncond_foreign);
        full.push_str("}\n");
    }
    full.push_str(&cond_foreign);

    let parsed =
        syn::parse_file(&full).unwrap_or_else(|e| panic!("xtask merge: assembled output failed to parse: {e}"));
    let pretty = prettyplease::unparse(&parsed);

    // This file is `include!`d by `src/sys.rs`, so it cannot carry its own inner attributes
    // (`#![allow(...)]`) — an included file's inner attributes are rejected by rustc. The lints
    // that generated FFI trips (rustc `unnecessary_transmutes`; clippy `missing_safety_doc`,
    // `useless_transmute`, `ptr_offset_with_cast`, `type_complexity`) are therefore allowed at the
    // include site in `src/sys.rs`. Keep that list in sync when bindgen starts emitting new shapes.
    let header = "// @generated by `cargo run -p xtask -- merge`. DO NOT EDIT BY HAND.\n\
                  // Comprehensive FFmpeg bindings, unioned by `xtask` across the CI OS matrix.\n\
                  // Until the bindings workflow has run on all OSes this may cover fewer targets.\n\
                  //\n\
                  // Lint suppression for this generated FFI lives at the `include!` site in\n\
                  // `src/sys.rs` (an included file cannot carry its own `#![allow(...)]`).\n\n";
    std::fs::write(out, format!("{header}{pretty}"))
        .unwrap_or_else(|e| panic!("xtask merge: cannot write {}: {e}", out.display()));
    eprintln!("xtask merge: wrote {} ({} items)", out.display(), order.len());
}

fn push(
    order: &mut Vec<String>,
    map: &mut HashMap<String, Entry>,
    key: String,
    tokens: String,
    os: &str,
    is_foreign: bool,
) {
    if !map.contains_key(&key) {
        order.push(key.clone());
        map.insert(
            key.clone(),
            Entry {
                is_foreign,
                variants: Vec::new(),
            },
        );
    }
    let entry = map.get_mut(&key).unwrap();
    match entry.variants.iter_mut().find(|v| v.tokens == tokens) {
        Some(v) => {
            if !v.oses.iter().any(|o| o == os) {
                v.oses.push(os.to_string());
            }
        }
        None => entry.variants.push(Variant {
            tokens,
            oses: vec![os.to_string()],
        }),
    }
}

fn cfg_attr(oses: &[String]) -> String {
    let preds: Vec<String> = oses.iter().map(|o| format!("target_os = \"{o}\"")).collect();
    if preds.len() == 1 {
        format!("#[cfg({})]", preds[0])
    } else {
        format!("#[cfg(any({}))]", preds.join(", "))
    }
}

/// The complement of [`cfg_attr`]: matches every OS *except* those in `oses`. Used for the
/// fallback arm of a conflicting item so the symbol is also defined on hosts none of the
/// real variants covered.
fn cfg_not_any(oses: &[String]) -> String {
    let preds: Vec<String> = oses.iter().map(|o| format!("target_os = \"{o}\"")).collect();
    format!("#[cfg(not(any({})))]", preds.join(", "))
}

fn item_key_tokens(item: &Item) -> (String, String) {
    let tokens = quote!(#item).to_string();
    let key = match item {
        Item::Struct(i) => format!("ty:{}", i.ident),
        Item::Enum(i) => format!("ty:{}", i.ident),
        Item::Union(i) => format!("ty:{}", i.ident),
        Item::Type(i) => format!("ty:{}", i.ident),
        Item::Const(i) => format!("const:{}", i.ident),
        Item::Static(i) => format!("static:{}", i.ident),
        Item::Fn(i) => format!("fn:{}", i.sig.ident),
        Item::Impl(i) => {
            // Key by self-type + trait + where-clause + method-name set so that two
            // *distinct* impls for the same type (e.g. bindgen's two
            // `impl __BindgenBitfieldUnit` blocks) get distinct keys, while an identical
            // impl produced by several OSes still dedups. A same-signature/different-body
            // impl across OSes remains a detected conflict (and is cfg-gated).
            let ty = &i.self_ty;
            let tr = i
                .trait_
                .as_ref()
                .map(|(_, p, _)| quote!(#p).to_string())
                .unwrap_or_default();
            let wc = i
                .generics
                .where_clause
                .as_ref()
                .map(|w| quote!(#w).to_string())
                .unwrap_or_default();
            let mut methods: Vec<String> = i
                .items
                .iter()
                .filter_map(|it| match it {
                    syn::ImplItem::Fn(f) => Some(f.sig.ident.to_string()),
                    syn::ImplItem::Const(c) => Some(c.ident.to_string()),
                    _ => None,
                })
                .collect();
            methods.sort();
            format!("impl:{tr}:{}:{}:{}", quote!(#ty), wc, methods.join(","))
        }
        Item::Mod(i) => format!("mod:{}", i.ident),
        Item::Use(_) => format!("use:{tokens}"),
        _ => format!("misc:{tokens}"),
    };
    (key, tokens)
}

fn foreign_key_tokens(fi: &ForeignItem) -> (String, String) {
    let tokens = quote!(#fi).to_string();
    let key = match fi {
        ForeignItem::Fn(f) => format!("fn:{}", f.sig.ident),
        ForeignItem::Static(s) => format!("static:{}", s.ident),
        ForeignItem::Type(t) => format!("ty:{}", t.ident),
        _ => format!("fmisc:{tokens}"),
    };
    (key, tokens)
}
