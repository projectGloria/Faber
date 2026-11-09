//! Stack detection. Every detector inspects the folder and may claim it with a priority;
//! the most specific claim wins (Tauri > Electron > custom script > .NET > Cargo > Python > Node),
//! and the losers are reported as `alternatives` so the UI can show what else was seen.
//!
//! Everything here is plain filesystem reads, so it's unit-testable against temp dirs.

mod cargo;
mod cmake;
mod dotnet;
mod electron;
mod extension;
mod flutter;
mod go_lang;
mod gradle;
mod node;
mod python;
mod script;
mod tauri_app;
#[cfg(test)]
mod tests;
mod zig;

pub use node::PackageManager;

use crate::plan::{BuildPlan, Step, StepKind};
use crate::toolchain;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::path::{Path, PathBuf};
use std::time::SystemTime;

#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum Stack {
    Tauri,
    Electron,
    Flutter,
    Dart,
    Gradle,
    Go,
    Cmake,
    Zig,
    Extension,
    Node,
    Cargo,
    Dotnet,
    Msbuild,
    Python,
    Script,
}

/// Per-project knobs from the UI (persisted there per path).
#[derive(Deserialize, Clone, Debug)]
#[serde(rename_all = "camelCase", default)]
pub struct Options {
    pub portable: bool,
    pub installer: bool,
    pub skip_install: bool,
    pub force_install: bool,
    /// Whitespace-separated args appended to the main build step.
    pub extra_args: String,
}

impl Default for Options {
    fn default() -> Self {
        Options { portable: true, installer: true, skip_install: false, force_install: false, extra_args: String::new() }
    }
}

#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum Level {
    Info,
    Warn,
    Error,
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub struct Note {
    pub level: Level,
    pub text: String,
}

impl Note {
    pub fn info(t: impl Into<String>) -> Self { Note { level: Level::Info, text: t.into() } }
    pub fn warn(t: impl Into<String>) -> Self { Note { level: Level::Warn, text: t.into() } }
    pub fn error(t: impl Into<String>) -> Self { Note { level: Level::Error, text: t.into() } }
}

#[derive(Serialize, Deserialize, Clone, Debug)]
#[serde(rename_all = "camelCase")]
pub struct Detection {
    pub root: String,
    pub name: String,
    pub stack: Option<Stack>,
    pub tags: Vec<String>,
    pub package_manager: Option<PackageManager>,
    pub plan: BuildPlan,
    pub notes: Vec<Note>,
    pub alternatives: Vec<Stack>,
    /// Build folders that exist right now and would be removed (root-relative, `/`-separated).
    pub cleanable: Vec<String>,
}

/// What a single detector claims.
pub struct Detected {
    pub stack: Stack,
    pub priority: u8,
    pub tags: Vec<String>,
    pub pm: Option<PackageManager>,
    pub steps: Vec<Step>,
    pub artifacts: Vec<String>,
    pub pinned_artifacts: Vec<String>,
    pub clean: Vec<String>,
    pub notes: Vec<Note>,
    /// Bare program names that must be on PATH; missing ones become warnings.
    pub tools: Vec<String>,
}

impl Detected {
    pub fn new(stack: Stack, priority: u8) -> Self {
        Detected {
            stack,
            priority,
            tags: vec![],
            pm: None,
            steps: vec![],
            artifacts: vec![],
            pinned_artifacts: vec![],
            clean: vec![],
            notes: vec![],
            tools: vec![],
        }
    }
}

/// Read-only view of the project folder shared by all detectors.
pub struct Ctx {
    pub root: PathBuf,
    pub pkg: Option<Value>,
    pub opts: Options,
}

impl Ctx {
    pub fn new(root: &Path, opts: Options) -> Self {
        let pkg = std::fs::read_to_string(root.join("package.json"))
            .ok()
            .and_then(|s| serde_json::from_str(&s).ok());
        Ctx { root: root.to_path_buf(), pkg, opts }
    }
    pub fn p(&self, rel: &str) -> PathBuf { self.root.join(rel) }
    pub fn has(&self, rel: &str) -> bool { self.p(rel).exists() }
    pub fn read(&self, rel: &str) -> Option<String> { std::fs::read_to_string(self.p(rel)).ok() }
    pub fn glob(&self, pat: &str) -> Vec<PathBuf> { glob_in(&self.root, pat) }
    pub fn mtime(&self, rel: &str) -> Option<SystemTime> { mtime(&self.p(rel)) }

    pub fn script(&self, name: &str) -> Option<&str> {
        self.pkg.as_ref()?.get("scripts")?.get(name)?.as_str()
    }
    pub fn scripts(&self) -> Vec<(&str, &str)> {
        self.pkg
            .as_ref()
            .and_then(|p| p.get("scripts"))
            .and_then(Value::as_object)
            .map(|m| m.iter().filter_map(|(k, v)| Some((k.as_str(), v.as_str()?))).collect())
            .unwrap_or_default()
    }
    pub fn has_dep(&self, name: &str) -> bool {
        let Some(pkg) = &self.pkg else { return false };
        ["dependencies", "devDependencies"].iter().any(|k| pkg.get(k).and_then(|d| d.get(name)).is_some())
    }
    /// Common frontend tags so the UI can show "Vite · React · TS" badges.
    pub fn js_tags(&self) -> Vec<String> {
        let mut t = vec![];
        for (dep, tag) in [
            ("electron-vite", "electron-vite"),
            ("vite", "Vite"),
            ("react", "React"),
            ("vue", "Vue"),
            ("svelte", "Svelte"),
            ("next", "Next"),
            ("wxt", "WXT"),
            ("turbo", "turbo"),
            ("typescript", "TS"),
        ] {
            if self.has_dep(dep) {
                t.push(tag.to_string());
            }
        }
        t
    }
}

pub fn mtime(p: &Path) -> Option<SystemTime> {
    std::fs::metadata(p).and_then(|m| m.modified()).ok()
}

/// Glob relative to `root`. The root itself is escaped so folders like `[old] App` don't
/// turn into character classes; matching is case-insensitive like the Windows filesystem.
pub fn glob_in(root: &Path, pat: &str) -> Vec<PathBuf> {
    let base = glob::Pattern::escape(&root.to_string_lossy());
    let full = format!("{base}/{pat}");
    let opts = glob::MatchOptions { case_sensitive: false, ..Default::default() };
    glob::glob_with(&full, opts).map(|it| it.filter_map(Result::ok).collect()).unwrap_or_default()
}

/// Quote-aware argument splitter that preserves strings wrapped in single or double quotes.
pub fn split_args(input: &str) -> Vec<String> {
    let mut args = Vec::new();
    let mut current = String::new();
    let mut in_quotes = false;
    let mut quote_char = ' ';

    for c in input.chars() {
        match c {
            '"' | '\'' if !in_quotes => {
                in_quotes = true;
                quote_char = c;
            }
            c if in_quotes && c == quote_char => {
                in_quotes = false;
            }
            c if c.is_whitespace() && !in_quotes => {
                if !current.is_empty() {
                    args.push(std::mem::take(&mut current));
                }
            }
            _ => current.push(c),
        }
    }
    if !current.is_empty() {
        args.push(current);
    }
    args
}

pub fn detect(root: &Path, opts: Options) -> Detection {
    let ctx = Ctx::new(root, opts);
    let name = root.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default();

    let mut found: Vec<Detected> = [
        tauri_app::detect,
        electron::detect,
        flutter::detect,
        extension::detect,
        script::detect,
        gradle::detect,
        dotnet::detect,
        cargo::detect,
        zig::detect,
        go_lang::detect,
        python::detect,
        cmake::detect,
        node::detect,
    ]
    .iter()
    .filter_map(|d| d(&ctx))
    .collect();
    found.sort_by(|a, b| b.priority.cmp(&a.priority));

    let mut iter = found.into_iter();
    let Some(mut primary) = iter.next() else {
        return Detection {
            root: root.to_string_lossy().into_owned(),
            name,
            stack: None,
            tags: vec![],
            package_manager: None,
            plan: BuildPlan::default(),
            notes: vec![Note::error("No buildable project found here (looked for package.json, src-tauri, pubspec.yaml, Gradle, Cargo.toml, *.csproj/*.sln, Go, CMake, Zig, Python, build scripts).")],
            alternatives: vec![],
            cleanable: vec![],
        };
    };
    let alternatives: Vec<Stack> = iter.map(|d| d.stack).filter(|s| *s != primary.stack).collect();

    // Older Faber versions copied outputs into `faber-out/`; offer to sweep those leftovers too.
    primary.clean.push("faber-out".into());

    // Inno Setup script = a hand-made installer; honour it for stacks that don't bundle their own.
    let bundles_itself = matches!(primary.stack, Stack::Tauri | Stack::Electron);
    let iss_files: Vec<PathBuf> = ["*.iss", "installer/*.iss", "install/*.iss", "setup/*.iss"]
        .iter()
        .flat_map(|p| ctx.glob(p))
        .collect();
    if !bundles_itself && !iss_files.is_empty() {
        primary.clean.push("Output".into());
        primary.clean.push("dist".into());
    }
    if ctx.opts.installer && !bundles_itself {
        if let Some(iss) = iss_files.first() {
            match toolchain::iscc() {
                Some(iscc) => {
                    let iss_rel = crate::clean::rel(root, iss);
                    let mut iscc_args = vec![iss_rel.clone()];
                    if let Some(text) = ctx.read(&iss_rel) {
                        if text.contains("#ifndef AppVersion") || text.contains("{#AppVersion}") {
                            let ver = ctx.read("Cargo.toml")
                                .and_then(|t| {
                                    t.lines().find(|l| l.trim().starts_with("version =") || l.trim().starts_with("version="))
                                        .and_then(|l| l.split('"').nth(1))
                                        .map(String::from)
                                })
                                .or_else(|| {
                                    ctx.pkg.as_ref().and_then(|p| p.get("version")).and_then(|v| v.as_str()).map(String::from)
                                });
                            if let Some(v) = ver {
                                iscc_args.insert(0, format!("/DAppVersion={v}"));
                            }
                        }
                    }
                    primary.steps.push(Step::exec(
                        StepKind::Package,
                        "Inno Setup installer",
                        iscc.to_string_lossy(),
                        iscc_args,
                    ));
                    primary.artifacts.push("Output/*.exe".into());
                    primary.artifacts.push("dist/*.exe".into());
                }
                None => primary.notes.push(Note::warn("Found an .iss installer script but ISCC (Inno Setup 6) isn't installed.")),
            }
        }
    }

    // Extra args go to the last real build/package command, the one that matters.
    let extra = split_args(&ctx.opts.extra_args);
    if !extra.is_empty() {
        if let Some(step) = primary
            .steps
            .iter_mut()
            .rev()
            .find(|s| s.kind != StepKind::Install)
        {
            step.push_args(&extra);
        }
    }

    let mut seen_tools = std::collections::HashSet::new();
    primary.tools.retain(|t| seen_tools.insert(t.clone()));
    for tool in &primary.tools {
        match toolchain::command_for(tool) {
            None => primary.notes.push(Note::error(format!("`{tool}` isn't on PATH, so the build will fail until it's installed."))),
            Some((exe, prefix)) if !prefix.is_empty() => {
                let via = exe.file_stem().unwrap_or_default().to_string_lossy().into_owned();
                primary.notes.push(Note::info(format!("`{tool}` isn't installed globally, so Faber runs it through `{via} {}`.", prefix.join(" "))));
            }
            Some(_) => {}
        }
    }
    if !ctx.opts.portable && !ctx.opts.installer {
        primary.notes.push(Note::info("Portable and installer are both off, so only the build itself runs."));
    }

    let cleanable = crate::clean::resolve(root, &primary.clean).iter().map(|p| crate::clean::rel(root, p)).collect();
    let mut plan = BuildPlan {
        steps: primary.steps,
        artifacts: primary.artifacts,
        pinned_artifacts: primary.pinned_artifacts,
        clean: primary.clean,
    };
    plan.finalize();
    Detection {
        root: root.to_string_lossy().into_owned(),
        name,
        stack: Some(primary.stack),
        tags: primary.tags,
        package_manager: primary.pm,
        plan,
        notes: primary.notes,
        alternatives,
        cleanable,
    }
}
