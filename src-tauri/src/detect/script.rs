//! The project's own build script (build.ps1, Build-*.ps1, build.bat/cmd). If you wrote one,
//! it encodes knowledge no detector has, so it outranks the generic Python/.NET/Cargo rules.

use super::{Ctx, Detected, Note, Stack};
use crate::plan::{Step, StepKind, sv};
use std::path::PathBuf;

pub fn detect(ctx: &Ctx) -> Option<Detected> {
    let scripts: Vec<PathBuf> = ["build.ps1", "Build-*.ps1", "build.bat", "build.cmd"]
        .iter()
        .flat_map(|p| ctx.glob(p))
        .collect();
    let script = scripts.first()?;
    let file = script.file_name().unwrap_or_default().to_string_lossy().into_owned();

    let mut d = Detected::new(Stack::Script, 70);
    let is_ps = file.to_lowercase().ends_with(".ps1");
    let step = if is_ps {
        d.tags = vec!["PowerShell".into()];
        // Windows PowerShell 5.1, not pwsh: scripts like Build-Launcher.ps1 hard-require the Desktop edition.
        let mut args = sv(&["-NoProfile", "-ExecutionPolicy", "Bypass", "-File"]);
        args.push(file.clone()); // relative: cwd is the project root, keeps the command readable
        Step::exec(StepKind::Build, format!("Run {file}"), "powershell", args)
    } else {
        d.tags = vec!["Batch".into()];
        // Explicit `.\`: cmd skips the current directory when NoDefaultCurrentDirectoryInExePath is set.
        Step::exec(StepKind::Build, format!("Run {file}"), "cmd", vec!["/C".into(), format!(".\\{file}")])
    };
    d.steps.push(step);
    d.notes.push(Note::info(format!("Using your own build script `{file}`.")));
    if scripts.len() > 1 {
        let others: Vec<String> = scripts[1..].iter().map(|p| p.file_name().unwrap_or_default().to_string_lossy().into_owned()).collect();
        d.notes.push(Note::warn(format!("Other build scripts were ignored: {}", others.join(", "))));
    }
    // Scripts put output anywhere; the run-time mtime filter keeps this from listing stale files.
    d.artifacts = sv(&["*.exe", "dist/*", "release/*", "build/*.exe", "out/*.exe", "portable*/*"]);
    // Only the unambiguous output names: a script's `build/` or `out/` may well be hand-written sources.
    d.clean = sv(&["dist", "release"]);
    Some(d)
}
