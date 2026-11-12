//! Plain Rust crates/workspaces (not Tauri, which has its own detector).

use super::{Ctx, Detected, Note, Stack};
use crate::plan::{Step, StepKind, sv};
use crate::toolchain;

pub fn detect(ctx: &Ctx) -> Option<Detected> {
    if !ctx.has("Cargo.toml") {
        return None;
    }
    let has_tauri = ctx.has("src-tauri")
        || ctx.has("app/src-tauri")
        || ctx.has("desktop/src-tauri")
        || ctx.has("client/src-tauri");
    // If a tauri directory exists, lower cargo's priority so valid Tauri projects claim it,
    // but broken/incomplete Tauri checkouts still fall back to Cargo as primary or alternative.
    let priority = if has_tauri { 35 } else { 55 };
    let mut d = Detected::new(Stack::Cargo, priority);
    d.tags = vec!["Rust".into()];
    d.tools = vec!["cargo".into()];
    d.steps.push(Step::exec(StepKind::Build, "cargo build --release", "cargo", sv(&["build", "--release"])));
    d.artifacts.push("target/release/*.exe".into());
    d.clean.push("target".into());

    if ctx.opts.installer {
        // cargo-wix is the de-facto installer path for plain crates; only used if the project opted in.
        if ctx.has("wix/main.wxs") && toolchain::exists("cargo-wix") {
            d.steps.push(Step::exec(StepKind::Package, "MSI (cargo-wix)", "cargo", sv(&["wix", "--nocapture"])));
            d.artifacts.push("target/wix/*.msi".into());
        } else if ctx.glob("*.iss").is_empty() {
            d.notes.push(Note::info("Plain Rust crate, so the release exe is the portable build. No installer is configured (no wix/ or .iss)."));
        }
    }
    Some(d)
}

/// `[workspace]` section present, so `target/` lives next to this Cargo.toml.
pub fn is_workspace(toml: &str) -> bool {
    toml.lines().any(|l| l.trim() == "[workspace]")
}

/// Minimal `[package] name = "..."` extraction; no TOML crate needed for one field.
pub fn package_name(toml: &str) -> Option<String> {
    let mut in_pkg = false;
    for line in toml.lines().map(str::trim) {
        if line.starts_with('[') {
            in_pkg = line == "[package]";
            continue;
        }
        if in_pkg {
            if let Some((k, v)) = line.split_once('=') {
                if k.trim() == "name" {
                    let val = v.split('#').next().unwrap_or("").trim().trim_matches(|c| c == '"' || c == '\'');
                    return Some(val.to_string());
                }
            }
        }
    }
    None
}
