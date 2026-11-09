//! The project's own build script (build.ps1, Build-*.ps1, build.bat/cmd). If you wrote one,
//! it encodes knowledge no detector has, so it outranks the generic Python/.NET/Cargo rules.

use super::{Ctx, Detected, Note, Stack};
use crate::plan::{Step, StepKind, sv};
use std::path::PathBuf;

pub fn detect(ctx: &Ctx) -> Option<Detected> {
    let patterns = [
        "build.ps1",
        "Build-*.ps1",
        "build-*.ps1",
        "build.bat",
        "build-*.bat",
        "build.cmd",
        "build-*.cmd",
        "scripts/build.ps1",
        "scripts/build-*.ps1",
        "scripts/build.bat",
        "scripts/build-*.bat",
        "scripts/build.cmd",
        "scripts/build-*.cmd",
        "installer/build.ps1",
        "installer/build.bat",
        "installer/build.cmd",
    ];
    let scripts: Vec<PathBuf> = patterns
        .iter()
        .flat_map(|p| ctx.glob(p))
        .collect();
    let script = scripts.first()?;
    let rel_path = crate::clean::rel(&ctx.root, script).replace('/', "\\");
    let file = script.file_name().unwrap_or_default().to_string_lossy().into_owned();

    let mut d = Detected::new(Stack::Script, 70);
    let is_ps = file.to_lowercase().ends_with(".ps1");
    let step = if is_ps {
        d.tags = vec!["PowerShell".into()];
        // Windows PowerShell 5.1, not pwsh: scripts like Build-Launcher.ps1 hard-require the Desktop edition.
        let mut args = sv(&["-NoProfile", "-ExecutionPolicy", "Bypass", "-File"]);
        args.push(rel_path.clone()); // relative to project root
        Step::exec(StepKind::Build, format!("Run {file}"), "powershell", args)
    } else {
        d.tags = vec!["Batch".into()];
        // Explicit `.\`: cmd skips the current directory when NoDefaultCurrentDirectoryInExePath is set.
        Step::exec(StepKind::Build, format!("Run {file}"), "cmd", vec!["/C".into(), format!(".\\{rel_path}")])
    };
    d.steps.push(step);
    d.notes.push(Note::info(format!("Using your own build script `{rel_path}`.")));
    if scripts.len() > 1 {
        let others: Vec<String> = scripts[1..].iter().map(|p| crate::clean::rel(&ctx.root, p)).collect();
        d.notes.push(Note::warn(format!("Other build scripts were ignored: {}", others.join(", "))));
    }
    // Scripts put output anywhere; the run-time mtime filter keeps this from listing stale files.
    d.artifacts = sv(&[
        "*.exe",
        "dist/*",
        "dist/*/*",
        "release/*",
        "release/*/*",
        "build/*.exe",
        "out/*.exe",
        "portable*/*",
        "target/release/*.exe",
        "target/release/bundle/*/*",
    ]);
    // Only the unambiguous output names: a script's `build/` or `out/` may well be hand-written sources.
    d.clean = sv(&["dist", "release"]);
    Some(d)
}
