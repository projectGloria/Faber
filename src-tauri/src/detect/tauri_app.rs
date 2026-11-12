//! Tauri v2 apps. `npm run build` here only builds the Vite frontend; the real artifacts come
//! from `tauri build`: NSIS/MSI installers under bundle/, plus the raw release exe, which
//! is self-contained on Windows (WebView2 ships with the OS) and works as a portable build.
//! Everything stays where Tauri puts it (`target/release/…`); Faber doesn't copy into its own folder.

use super::{Ctx, Detected, Note, PackageManager, Stack, cargo};
use crate::plan::{Step, StepKind};
use serde_json::Value;

pub fn detect(ctx: &Ctx) -> Option<Detected> {
    let candidates = [
        "src-tauri",
        "app/src-tauri",
        "desktop/src-tauri",
        "client/src-tauri",
    ];
    let (tauri_dir, conf_rel) = candidates
        .iter()
        .find_map(|dir| {
            for name in &["tauri.conf.json", "tauri.conf.json5", "Tauri.toml"] {
                let rel = format!("{dir}/{name}");
                if ctx.has(&rel) {
                    return Some((*dir, rel));
                }
            }
            None
        })?;

    let conf: Value = ctx
        .read(&conf_rel)
        .filter(|_| conf_rel.ends_with(".json"))
        .and_then(|s| serde_json::from_str(&s).ok())
        .unwrap_or(Value::Null);

    // An explicit target list in the config is a deliberate choice, so don't override it.
    // Handles both Tauri v2 (/bundle/targets) and Tauri v1 (/tauri/bundle/targets).
    let explicit_targets = matches!(conf.pointer("/bundle/targets"), Some(Value::Array(_)))
        || matches!(conf.pointer("/bundle/targets"), Some(Value::String(s)) if s != "all")
        || matches!(conf.pointer("/tauri/bundle/targets"), Some(Value::Array(_)))
        || matches!(conf.pointer("/tauri/bundle/targets"), Some(Value::String(s)) if s != "all");

    let subfolder = tauri_dir.strip_suffix("/src-tauri");
    let has_subfolder_pkg = subfolder.is_some_and(|sub| ctx.has(&format!("{sub}/package.json")));

    let mut d = Detected::new(Stack::Tauri, 90);
    d.tags = std::iter::once("Rust".to_string()).chain(ctx.js_tags()).collect();
    d.tools.push("cargo".into());

    let (pm, cwd, has_sub_tauri_script, has_sub_tauri_cli) = if has_subfolder_pkg {
        let sub = subfolder.unwrap();
        let sub_ctx = Ctx::new(&ctx.root.join(sub), ctx.opts.clone());
        let pm = sub_ctx.pkg.as_ref().map(|_| PackageManager::detect(&sub_ctx));
        if let Some(pm) = pm {
            d.pm = Some(pm);
            d.tools.insert(0, pm.bin().into());
            if let Some(mut step) = super::node::install_step(&sub_ctx, pm) {
                step = step.with_cwd(sub);
                d.steps.push(step);
            }
        }
        (pm, Some(sub.to_string()), sub_ctx.script("tauri").is_some(), sub_ctx.has_dep("@tauri-apps/cli"))
    } else {
        let pm = ctx.pkg.as_ref().map(|_| PackageManager::detect(ctx));
        if let Some(pm) = pm {
            d.pm = Some(pm);
            d.tools.insert(0, pm.bin().into());
            d.steps.extend(super::node::install_step(ctx, pm));
        }
        (pm, None, false, false)
    };

    let mut args: Vec<&str> = vec!["build"];
    if !ctx.opts.installer {
        args.push("--no-bundle");
    } else if !explicit_targets {
        args.extend(["--bundles", "nsis,msi"]);
    }
    let (prog, argv) = match pm {
        Some(pm) if ctx.script("tauri").is_some() || has_sub_tauri_script => pm.run("tauri", &args),
        Some(pm) if ctx.has_dep("@tauri-apps/cli") || has_sub_tauri_cli => pm.exec("tauri", &args),
        _ => {
            d.notes.push(Note::warn("No @tauri-apps/cli in package.json, so Faber falls back to `cargo tauri` (needs `cargo install tauri-cli`)."));
            let mut a = vec!["tauri".to_string()];
            a.extend(args.iter().map(|s| s.to_string()));
            ("cargo".to_string(), a)
        }
    };
    let label = if ctx.opts.installer { "Tauri build + bundle" } else { "Tauri build" };
    let mut build_step = Step::exec(StepKind::Build, label, prog, argv);
    if let Some(c) = cwd {
        build_step = build_step.with_cwd(c);
    }
    d.steps.push(build_step);

    // A root Cargo workspace (e.g. OmniConvert, devCleanup) moves `target/` up to the repo root.
    let root_is_workspace = ctx.read("Cargo.toml").is_some_and(|t| cargo::is_workspace(&t));
    let target = if root_is_workspace { "target".to_string() } else { format!("{tauri_dir}/target") };
    let bin = conf
        .get("mainBinaryName")
        .and_then(Value::as_str)
        .map(String::from)
        .or_else(|| ctx.read(&format!("{tauri_dir}/Cargo.toml")).and_then(|t| cargo::package_name(&t)))
        .or_else(|| {
            conf.pointer("/package/productName")
                .or_else(|| conf.pointer("/productName"))
                .and_then(Value::as_str)
                .map(String::from)
        });

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
    d.clean.push(target);
    let dist_folder = frontend_dist(&conf).unwrap_or_else(|| "dist".into());
    let clean_dist = if let Some(sub) = subfolder {
        format!("{sub}/{dist_folder}")
    } else {
        dist_folder
    };
    d.clean.push(clean_dist);
    Some(d)
}

/// `build.frontendDist` (v2) or `build.distDir` (v1) is relative to src-tauri (`"../dist"`);
/// dev-server URLs and paths outside the project root aren't build output Faber should touch.
fn frontend_dist(conf: &Value) -> Option<String> {
    let v = conf.pointer("/build/frontendDist")
        .or_else(|| conf.pointer("/build/distDir"))?
        .as_str()?;
    let rel = v.strip_prefix("../")?.trim_end_matches('/');
    (!rel.is_empty() && !rel.contains("..")).then(|| rel.to_string())
}
