//! VS Code Extensions (.vsix) and Browser WebExtensions (.zip).
//! Packages VS Code extensions with their custom packaging scripts or `@vscode/vsce package`,
//! and compresses WebExtensions into zip packages.

use super::node::{PackageManager, install_step, script_step};
use super::{Ctx, Detected, Stack};
use crate::plan::{Step, StepKind, sv};

pub fn detect(ctx: &Ctx) -> Option<Detected> {
    // 1. VS Code Extension
    let is_vscode = ctx
        .pkg
        .as_ref()
        .and_then(|p| p.get("engines"))
        .and_then(|e| e.get("vscode"))
        .is_some();

    if is_vscode {
        let pm = PackageManager::detect(ctx);
        let mut d = Detected::new(Stack::Extension, 72);
        d.pm = Some(pm);
        d.tags = std::iter::once("VS Code Extension".to_string())
            .chain(ctx.js_tags())
            .collect();
        d.tools = vec![pm.bin().into()];
        d.steps.extend(install_step(ctx, pm));

        if ctx.script("package").is_some() {
            d.steps.push(script_step(pm, StepKind::Package, "Package VSIX", "package", &[]));
        } else {
            if ctx.script("vscode:prepublish").is_some() {
                d.steps.push(script_step(pm, StepKind::Build, "Prepublish", "vscode:prepublish", &[]));
            } else if ctx.script("build").is_some() {
                d.steps.push(script_step(pm, StepKind::Build, "Build", "build", &[]));
            }
            let (prog, argv) = pm.exec("@vscode/vsce", &["package", "--no-dependencies"]);
            d.steps.push(Step::exec(StepKind::Package, "vsce package", prog, argv));
        }

        d.artifacts = sv(&["*.vsix"]);
        d.clean = sv(&["dist", "out", "*.vsix"]);
        return Some(d);
    }

    // 2. Browser WebExtension
    let manifest = ctx.read("manifest.json");
    let is_webext = manifest.as_ref().is_some_and(|m| m.contains("manifest_version"));

    if is_webext && !ctx.has_dep("wxt") {
        let mut d = Detected::new(Stack::Extension, 45);
        d.tags = vec!["WebExtension".into()];

        let pm = ctx.pkg.as_ref().map(|_| PackageManager::detect(ctx));
        if let Some(pm) = pm {
            d.pm = Some(pm);
            d.tools.push(pm.bin().into());
            d.steps.extend(install_step(ctx, pm));
            if ctx.script("build").is_some() {
                d.steps.push(script_step(pm, StepKind::Build, "Build extension", "build", &[]));
            }
        }

        let zip_cmd = "$name = (Get-Item .).Name; $files = Get-ChildItem -File | Where-Object { $_.Extension -ne '.zip' }; $dirs = Get-ChildItem -Directory | Where-Object { $_.Name -notin @('.git', 'node_modules') }; Compress-Archive -Path ($files + $dirs) -DestinationPath ($name + '.zip') -Force";
        d.steps.push(Step::exec(
            StepKind::Package,
            "Package Extension ZIP",
            "powershell",
            sv(&["-NoProfile", "-Command", zip_cmd]),
        ));

        d.artifacts = sv(&["*.zip", "dist/*.zip", "release/*.zip"]);
        d.clean = sv(&["dist", "*.zip"]);
        return Some(d);
    }

    None
}
