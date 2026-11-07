//! Tauri v2 apps. `npm run build` here only builds the Vite frontend; the real artifacts come
//! from `tauri build`: NSIS/MSI installers under bundle/, plus the raw release exe, which
//! is self-contained on Windows (WebView2 ships with the OS) and works as a portable build.
//! Everything stays where Tauri puts it (`target/release/…`); Faber doesn't copy into its own folder.

use super::{Ctx, Detected, Note, PackageManager, Stack, cargo};
use crate::plan::{Step, StepKind};
use serde_json::Value;

pub fn detect(ctx: &Ctx) -> Option<Detected> {
    let conf_rel = ["src-tauri/tauri.conf.json", "src-tauri/tauri.conf.json5", "src-tauri/Tauri.toml"]
        .into_iter()
        .find(|r| ctx.has(r))?;
    let conf: Value = ctx
        .read(conf_rel)
        .filter(|_| conf_rel.ends_with(".json"))
        .and_then(|s| serde_json::from_str(&s).ok())
        .unwrap_or(Value::Null);

    // An explicit target list in the config is a deliberate choice, so don't override it.
    let explicit_targets = matches!(conf.pointer("/bundle/targets"), Some(Value::Array(_)))
        || matches!(conf.pointer("/bundle/targets"), Some(Value::String(s)) if s != "all");

    let mut d = Detected::new(Stack::Tauri, 90);
    d.tags = std::iter::once("Rust".to_string()).chain(ctx.js_tags()).collect();
    d.tools.push("cargo".into());

    let pm = ctx.pkg.as_ref().map(|_| PackageManager::detect(ctx));
    if let Some(pm) = pm {
        d.pm = Some(pm);
        d.tools.insert(0, pm.bin().into());
        d.steps.extend(super::node::install_step(ctx, pm));
    }

    let mut args: Vec<&str> = vec!["build"];
    if !ctx.opts.installer {
        args.push("--no-bundle");
    } else if !explicit_targets {
        args.extend(["--bundles", "nsis,msi"]);
    }
    let (prog, argv) = match pm {
        Some(pm) if ctx.script("tauri").is_some() => pm.run("tauri", &args),
        Some(pm) if ctx.has_dep("@tauri-apps/cli") => pm.exec("tauri", &args),
        _ => {
            d.notes.push(Note::warn("No @tauri-apps/cli in package.json, so Faber falls back to `cargo tauri` (needs `cargo install tauri-cli`)."));
            let mut a = vec!["tauri".to_string()];
            a.extend(args.iter().map(|s| s.to_string()));
            ("cargo".to_string(), a)
        }
    };
    let label = if ctx.opts.installer { "Tauri build + bundle" } else { "Tauri build" };
    d.steps.push(Step::exec(StepKind::Build, label, prog, argv));

    // A root Cargo workspace (e.g. OmniConvert) moves `target/` up to the repo root.
    let root_is_workspace = ctx.read("Cargo.toml").is_some_and(|t| cargo::is_workspace(&t));
    let target = if root_is_workspace { "target" } else { "src-tauri/target" };
    let bin = conf
        .get("mainBinaryName")
        .and_then(Value::as_str)
        .map(String::from)
        .or_else(|| ctx.read("src-tauri/Cargo.toml").and_then(|t| cargo::package_name(&t)));

    if ctx.opts.portable {
        // The raw release exe *is* the portable build. Pinned (not mtime-filtered): when nothing changed
        // cargo skips relinking, so the exe keeps an old mtime yet is verified current by this very build.
        match &bin {
            Some(b) => d.pinned_artifacts.push(format!("{target}/release/{b}.exe")),
            None => d.artifacts.push(format!("{target}/release/*.exe")),
        }
    }
    if ctx.opts.installer {
        d.artifacts.push(format!("{target}/release/bundle/nsis/*.exe"));
        d.artifacts.push(format!("{target}/release/bundle/msi/*.msi"));
        if explicit_targets {
            d.notes.push(Note::info("tauri.conf.json pins bundle targets, so Faber uses those as-is."));
        }
    }

    // Whole target dir (debug + release + bundles) plus the frontend output Tauri embeds.
    d.clean.push(target.into());
    d.clean.push(frontend_dist(&conf).unwrap_or_else(|| "dist".into()));
    Some(d)
}

/// `build.frontendDist` is relative to src-tauri (`"../dist"`); dev-server URLs and paths
/// outside the project root aren't build output Faber should touch.
fn frontend_dist(conf: &Value) -> Option<String> {
    let v = conf.pointer("/build/frontendDist")?.as_str()?;
    let rel = v.strip_prefix("../")?.trim_end_matches('/');
    (!rel.is_empty() && !rel.contains("..")).then(|| rel.to_string())
}
