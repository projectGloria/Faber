//! "Remove uninstallers": deletes a project's build output folders (target/, dist/, bin+obj, …).
//! Detectors only *declare* glob patterns; everything that decides what actually gets deleted
//! lives here, so one bad pattern (or a hostile electron-builder `output: ..`) can't escape the root.

use crate::detect::glob_in;
use serde::Serialize;
use std::path::{Component, Path, PathBuf};

/// Never deleted, nor anything beneath them: dependencies and VCS state aren't build output.
const NEVER_INSIDE: &[&str] = &["node_modules", ".git", ".hg", ".svn", ".venv", "venv"];
/// Source-ish folders a sloppy pattern could hit (`*/bin` vs a real `src/`); refused as targets.
const NEVER_TARGET: &[&str] = &["src", "src-tauri", "public", "assets", "resources", "icons"];

#[derive(Serialize, Debug)]
#[serde(rename_all = "camelCase")]
pub struct CleanFailure {
    pub path: String,
    pub error: String,
}

#[derive(Serialize, Debug, Default)]
#[serde(rename_all = "camelCase")]
pub struct CleanReport {
    pub removed: Vec<String>,
    pub failed: Vec<CleanFailure>,
}

/// Existing, safe-to-delete directories matching `patterns`, deduped, in pattern order.
pub fn resolve(root: &Path, patterns: &[String]) -> Vec<PathBuf> {
    let mut out: Vec<PathBuf> = vec![];
    for pat in patterns {
        for p in glob_in(root, pat) {
            if is_safe(root, &p) && !out.contains(&p) {
                out.push(p);
            }
        }
    }
    // `target` already covers `target/release`: deleting the child first is wasted work and
    // makes the parent's removal report a confusing "not found" for its own subtree.
    let all = out.clone();
    out.retain(|p| !all.iter().any(|q| q != p && p.starts_with(q)));
    out
}

/// Root-relative, `/`-separated, for display and for the report.
pub fn rel(root: &Path, p: &Path) -> String {
    p.strip_prefix(root).unwrap_or(p).to_string_lossy().replace('\\', "/")
}

pub fn remove(root: &Path, patterns: &[String]) -> CleanReport {
    let mut report = CleanReport::default();
    for p in resolve(root, patterns) {
        let path = rel(root, &p);
        match std::fs::remove_dir_all(&p) {
            Ok(()) => report.removed.push(path),
            // Typical cause: the built exe is still running, or an editor/AV holds a handle.
            Err(e) => report.failed.push(CleanFailure { path, error: e.to_string() }),
        }
    }
    report
}

fn is_safe(root: &Path, p: &Path) -> bool {
    // symlink_metadata, not metadata: a symlink/junction (pnpm, `npm link`) must never be followed
    // out of the project. std reports Windows junctions as symlinks too.
    let Ok(meta) = std::fs::symlink_metadata(p) else { return false };
    if !meta.is_dir() || meta.file_type().is_symlink() {
        return false;
    }
    // strip_prefix is lexical, so `root/../x` would pass; requiring plain Normal components rejects
    // `..`, drive prefixes and the root itself (empty rel) in one go.
    let Ok(rel) = p.strip_prefix(root) else { return false };
    let names: Vec<String> = match rel.components().map(|c| match c {
        Component::Normal(s) => Some(s.to_string_lossy().to_lowercase()),
        _ => None,
    }).collect::<Option<Vec<_>>>() {
        Some(n) if !n.is_empty() => n,
        _ => return false,
    };
    if names.iter().any(|n| NEVER_INSIDE.contains(&n.as_str())) {
        return false;
    }
    !NEVER_TARGET.contains(&names.last().unwrap().as_str())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    fn mk(root: &Path, rel: &str) {
        fs::create_dir_all(root.join(rel)).unwrap();
        fs::write(root.join(rel).join("f.bin"), b"x").unwrap();
    }
    fn pats(p: &[&str]) -> Vec<String> {
        p.iter().map(|s| s.to_string()).collect()
    }

    #[test]
    fn resolves_only_existing_dirs_and_collapses_nested() {
        let t = tempfile::tempdir().unwrap();
        let root = dunce::canonicalize(t.path()).unwrap();
        mk(&root, "target/release");
        fs::write(root.join("dist"), b"a file, not a dir").unwrap();
        let got: Vec<String> = resolve(&root, &pats(&["target/release", "target", "dist", "out"])).iter().map(|p| rel(&root, p)).collect();
        assert_eq!(got, vec!["target"]);
    }

    #[test]
    fn refuses_escapes_deps_and_sources() {
        let t = tempfile::tempdir().unwrap();
        let root = dunce::canonicalize(t.path()).unwrap().join("proj");
        mk(&root, "node_modules/bin");
        mk(&root, "src");
        mk(&root, "obj");
        mk(root.parent().unwrap(), "outside");
        let got: Vec<String> = resolve(&root, &pats(&["../outside", ".", "", "*/bin", "src", "obj"])).iter().map(|p| rel(&root, p)).collect();
        assert_eq!(got, vec!["obj"]);
        assert!(root.parent().unwrap().join("outside").exists());
    }

    #[test]
    fn remove_deletes_and_reports() {
        let t = tempfile::tempdir().unwrap();
        let root = dunce::canonicalize(t.path()).unwrap();
        mk(&root, "bin/Release");
        mk(&root, "obj");
        mk(&root, "keep");
        let r = remove(&root, &pats(&["bin", "obj"]));
        assert_eq!(r.removed, vec!["bin", "obj"]);
        assert!(r.failed.is_empty());
        assert!(!root.join("bin").exists() && !root.join("obj").exists() && root.join("keep").exists());
    }
}
