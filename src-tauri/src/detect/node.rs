//! Package-manager handling shared by every JS-based stack, plus the generic Node detector
//! (Vite apps, WXT extensions, turbo monorepos, plain scripts).

use super::{Ctx, Detected, Note, Stack};
use crate::plan::{Step, StepKind, sv};
use serde::{Deserialize, Serialize};

#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum PackageManager {
    Npm,
    Pnpm,
    Yarn,
    Bun,
}

impl PackageManager {
    pub fn detect(ctx: &Ctx) -> Self {
        // Corepack's `packageManager` field is the explicit declaration, so it beats lockfile sniffing.
        if let Some(pm) = ctx.pkg.as_ref().and_then(|p| p.get("packageManager")).and_then(|v| v.as_str()) {
            for (prefix, m) in [("pnpm", Self::Pnpm), ("yarn", Self::Yarn), ("bun", Self::Bun), ("npm", Self::Npm)] {
                if pm.starts_with(prefix) {
                    return m;
                }
            }
        }
        if ctx.has("pnpm-lock.yaml") {
            Self::Pnpm
        } else if ctx.has("yarn.lock") {
            Self::Yarn
        } else if ctx.has("bun.lock") || ctx.has("bun.lockb") {
            Self::Bun
        } else {
            Self::Npm
        }
    }

    pub fn bin(self) -> &'static str {
        match self {
            Self::Npm => "npm",
            Self::Pnpm => "pnpm",
            Self::Yarn => "yarn",
            Self::Bun => "bun",
        }
    }

    fn lockfile(self) -> &'static str {
        match self {
            Self::Npm => "package-lock.json",
            Self::Pnpm => "pnpm-lock.yaml",
            Self::Yarn => "yarn.lock",
            Self::Bun => "bun.lock",
        }
    }

    /// File each PM rewrites inside node_modules on every install. Comparing its mtime
    /// to the lockfile tells us whether someone pulled new deps since the last install.
    fn install_marker(self) -> &'static str {
        match self {
            Self::Npm => "node_modules/.package-lock.json",
            Self::Pnpm => "node_modules/.modules.yaml",
            Self::Yarn => "node_modules/.yarn-integrity",
            Self::Bun => "node_modules",
        }
    }

    fn install_args(self) -> Vec<String> {
        match self {
            Self::Npm => sv(&["i"]),
            _ => sv(&["install"]),
        }
    }

    /// `npm run x -- args` needs the `--` separator, the others pass trailing args straight through.
    pub fn run(self, script: &str, extra: &[&str]) -> (String, Vec<String>) {
        let mut args = sv(&["run", script]);
        if !extra.is_empty() {
            if self == Self::Npm {
                args.push("--".into());
            }
            args.extend(extra.iter().map(|s| s.to_string()));
        }
        (self.bin().into(), args)
    }

    /// Run a package binary (`npx tauri`, `pnpm exec electron-builder`, ...).
    pub fn exec(self, bin: &str, extra: &[&str]) -> (String, Vec<String>) {
        let (prog, mut args) = match self {
            Self::Npm => ("npx", vec![]),
            Self::Pnpm => ("pnpm", sv(&["exec"])),
            Self::Yarn => ("yarn", vec![]),
            Self::Bun => ("bunx", vec![]),
        };
        args.push(bin.into());
        args.extend(extra.iter().map(|s| s.to_string()));
        (prog.into(), args)
    }
}

/// `None` = deps look current, so skip the install entirely (the whole point: no 40s `npm i` for nothing).
pub fn install_step(ctx: &Ctx, pm: PackageManager) -> Option<Step> {
    if ctx.opts.skip_install {
        return None;
    }
    let reason = if ctx.opts.force_install {
        "reinstall requested".to_string()
    } else if !ctx.has("node_modules") {
        "node_modules is missing".to_string()
    } else {
        let marker = ctx.mtime(pm.install_marker());
        let newer = |rel: &str| matches!((ctx.mtime(rel), marker), (Some(f), Some(m)) if f > m);
        if newer(pm.lockfile()) {
            format!("{} changed since the last install", pm.lockfile())
        } else if newer("package.json") {
            "package.json changed since the last install".to_string()
        } else {
            return None;
        }
    };
    let args = pm.install_args();
    let cmd = format!("{} {}", pm.bin(), args.join(" "));
    Some(
        Step::exec(StepKind::Install, "Install dependencies", pm.bin(), args)
            .reason(format!("{reason}, so Faber runs `{cmd}`")),
    )
}

pub fn script_step(pm: PackageManager, kind: StepKind, label: &str, script: &str, extra: &[&str]) -> Step {
    let (prog, args) = pm.run(script, extra);
    Step::exec(kind, label, prog, args)
}

pub fn detect(ctx: &Ctx) -> Option<Detected> {
    ctx.pkg.as_ref()?;
    let pm = PackageManager::detect(ctx);
    let mut d = Detected::new(Stack::Node, 40);
    d.pm = Some(pm);
    d.tags = ctx.js_tags();
    d.tools = vec![pm.bin().into()];
    d.steps.extend(install_step(ctx, pm));

    match ctx.script("build") {
        Some(_) => d.steps.push(script_step(pm, StepKind::Build, "Build", "build", &[])),
        None => d.notes.push(Note::warn("package.json has no `build` script, so only dependencies get installed.")),
    }
    if ctx.has_dep("wxt") && ctx.opts.installer && ctx.script("zip").is_some() {
        d.steps.push(script_step(pm, StepKind::Package, "Zip extension", "zip", &[]));
    }
    if ctx.opts.installer || ctx.opts.portable {
        d.notes.push(Note::info("A web/Node project has no portable exe or installer. The build output is the artifact."));
    }
    d.artifacts = sv(&["dist", "build", "out", ".output/*"]);
    d.clean = sv(&["dist", "out", ".output"]);
    // `build/` is only unambiguously output for CRA; elsewhere it's often scripts or icon sources.
    if ctx.has_dep("react-scripts") {
        d.clean.push("build".into());
    }
    Some(d)
}
