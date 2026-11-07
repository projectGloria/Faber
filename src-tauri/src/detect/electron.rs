//! Electron apps. Real projects already have a "dist"-style script that does the prebuild
//! (icons, typecheck, electron-vite) and then calls electron-builder, so Faber reuses it and
//! only adds `--win nsis portable` when it can be appended without breaking the chain.

use super::node::{PackageManager, install_step, script_step};
use super::{Ctx, Detected, Note, Stack};
use crate::plan::{Step, StepKind};

const PACKAGING_SCRIPTS: &[&str] = &["dist", "dist:win", "build:win", "package:win", "package", "release", "make", "build"];
const TARGET_WORDS: &[&str] = &["nsis", "portable", "msi", "zip", "7z", "appx", "squirrel", "dir", "--dir"];

pub fn detect(ctx: &Ctx) -> Option<Detected> {
    if !ctx.has_dep("electron") {
        return None;
    }
    let pm = PackageManager::detect(ctx);
    let mut d = Detected::new(Stack::Electron, 80);
    d.pm = Some(pm);
    d.tags = std::iter::once("Electron".to_string()).chain(ctx.js_tags()).collect();
    d.tools = vec![pm.bin().into()];
    d.steps.extend(install_step(ctx, pm));

    let targets: Vec<&str> = [(ctx.opts.installer, "nsis"), (ctx.opts.portable, "portable")]
        .into_iter()
        .filter_map(|(on, t)| on.then_some(t))
        .collect();

    if !ctx.has_dep("electron-builder") {
        // e.g. Kintsugi: a custom ts-node packaging pipeline. Run the most packaging-looking script.
        match ["package", "dist", "make", "build:prod", "build"].iter().find(|s| ctx.script(s).is_some()) {
            Some(s) => {
                d.steps.push(script_step(pm, StepKind::Build, &format!("Run `{s}`"), s, &[]));
                d.notes.push(Note::warn(format!("No electron-builder, so Faber runs your `{s}` script; portable/installer depend on what that script produces.")));
            }
            None => d.notes.push(Note::error("Electron app without electron-builder or a build script. Nothing to run.")),
        }
        d.artifacts = vec!["dist/*".into(), "out/*".into(), "release/*".into()];
        d.clean = vec!["dist".into(), "out".into(), "release".into()];
        return Some(d);
    }

    let out = output_dir(ctx);
    match packaging_script(ctx) {
        Some((name, body)) => {
            let last = body.split("&&").last().unwrap_or("").trim();
            let tokens: Vec<&str> = last.split_whitespace().collect();
            let last_is_builder = tokens.first() == Some(&"electron-builder");
            let pins = tokens.iter().skip(1).any(|t| TARGET_WORDS.contains(t));
            let ends_with_win = matches!(tokens.last(), Some(&("--win" | "-w" | "--windows")));
            let has_win = tokens.iter().any(|t| matches!(*t, "--win" | "-w" | "--windows"));

            // npm appends extra args to the *end* of the script string, so this only works if
            // electron-builder is the last command in the && chain.
            let extra: Option<Vec<&str>> = if !last_is_builder || pins || targets.is_empty() {
                None
            } else if ends_with_win {
                Some(targets.clone())
            } else if !has_win {
                Some(std::iter::once("--win").chain(targets.iter().copied()).collect())
            } else {
                None // `--win --x64`: can't splice targets after a non-target flag safely
            };

            match extra {
                Some(extra) => {
                    d.steps.push(script_step(pm, StepKind::Package, &format!("Package via `{name}`"), name, &extra));
                    d.notes.push(Note::info(format!("Using your `{name}` script with targets: {}.", targets.join(" + "))));
                }
                None => {
                    d.steps.push(script_step(pm, StepKind::Package, &format!("Package via `{name}`"), name, &[]));
                    d.notes.push(Note::info(format!("Using your `{name}` script unchanged (it sets its own targets/steps), so it may not produce both portable and installer.")));
                }
            }
        }
        None => {
            if ctx.script("build").is_some() {
                d.steps.push(script_step(pm, StepKind::Build, "Build", "build", &[]));
            }
            let mut args = vec!["--win"];
            args.extend(targets.iter().copied());
            let (prog, argv) = pm.exec("electron-builder", &args);
            d.steps.push(Step::exec(StepKind::Package, "electron-builder", prog, argv));
        }
    }
    d.artifacts = vec![format!("{out}/*.exe"), format!("{out}/*.msi")];
    d.clean.push(out);
    if ctx.has_dep("electron-vite") {
        d.clean.push("out".into()); // electron-vite's compiled main/preload/renderer
    }
    Some(d)
}

fn packaging_script<'a>(ctx: &'a Ctx) -> Option<(&'a str, &'a str)> {
    let calls_builder = |b: &str| b.contains("electron-builder") && !b.contains("--dir");
    for name in PACKAGING_SCRIPTS {
        if let Some(body) = ctx.script(name) {
            if calls_builder(body) {
                return ctx.scripts().into_iter().find(|(k, _)| k == name);
            }
        }
    }
    // Anything else that calls electron-builder (e.g. LucidaFetcher's dist:portable).
    ctx.scripts().into_iter().find(|(_, b)| calls_builder(b))
}

/// `directories.output` from package.json `build`, electron-builder.yml/.json, default `dist`.
fn output_dir(ctx: &Ctx) -> String {
    if let Some(o) = ctx.pkg.as_ref().and_then(|p| p.pointer("/build/directories/output")).and_then(|v| v.as_str()) {
        return o.trim_end_matches('/').to_string();
    }
    if let Some(json) = ctx.read("electron-builder.json").and_then(|s| serde_json::from_str::<serde_json::Value>(&s).ok()) {
        if let Some(o) = json.pointer("/directories/output").and_then(|v| v.as_str()) {
            return o.to_string();
        }
    }
    for yml in ["electron-builder.yml", "electron-builder.yaml"] {
        if let Some(text) = ctx.read(yml) {
            // Tiny YAML scan: an indented `output:` under a top-level `directories:` block.
            let mut in_dirs = false;
            for line in text.lines() {
                if !line.starts_with(' ') && !line.starts_with('\t') {
                    in_dirs = line.trim_end() == "directories:";
                } else if in_dirs {
                    if let Some(v) = line.trim().strip_prefix("output:") {
                        return v.trim().trim_matches(['"', '\'']).to_string();
                    }
                }
            }
        }
    }
    "dist".into()
}
