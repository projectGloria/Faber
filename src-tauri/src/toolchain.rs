//! Locating executables. `which` honours PATHEXT, which matters on Windows: `npm`, `pnpm`,
//! `yarn` and `npx` are `.cmd` shims, and std's Command only appends `.exe` on its own.

use std::path::{Path, PathBuf};
use std::process::Command;

pub fn resolve(program: &str) -> Option<PathBuf> {
    let p = Path::new(program);
    if p.is_absolute() {
        return p.exists().then(|| p.to_path_buf());
    }
    which::which(program).ok()
}

pub fn exists(program: &str) -> bool {
    resolve(program).is_some()
}

/// MSBuild is almost never on PATH; vswhere (installed with every VS / Build Tools since 2017)
/// knows where the latest instance put it.
pub fn msbuild() -> Option<PathBuf> {
    if let Ok(p) = which::which("msbuild") {
        return Some(p);
    }
    let pf86 = std::env::var("ProgramFiles(x86)").unwrap_or_else(|_| r"C:\Program Files (x86)".into());
    let vswhere = PathBuf::from(pf86).join(r"Microsoft Visual Studio\Installer\vswhere.exe");
    if !vswhere.exists() {
        return None;
    }
    let mut cmd = Command::new(vswhere);
    cmd.args([
        "-latest",
        "-products",
        "*",
        "-requires",
        "Microsoft.Component.MSBuild",
        "-find",
        r"MSBuild\**\Bin\MSBuild.exe",
    ]);
    no_window(&mut cmd);
    let out = cmd.output().ok()?;
    String::from_utf8_lossy(&out.stdout)
        .lines()
        .map(str::trim)
        .find(|l| !l.is_empty())
        .map(PathBuf::from)
        .filter(|p| p.exists())
}

/// Inno Setup compiler, for projects that ship an `.iss` installer script.
pub fn iscc() -> Option<PathBuf> {
    if let Ok(p) = which::which("iscc") {
        return Some(p);
    }
    ["ProgramFiles(x86)", "ProgramFiles"]
        .iter()
        .filter_map(|v| std::env::var(v).ok())
        .map(|pf| PathBuf::from(pf).join(r"Inno Setup 6\ISCC.exe"))
        .find(|p| p.exists())
}

/// CREATE_NO_WINDOW: without it every spawned console tool flashes a conhost window
/// in release builds (the GUI subsystem app has no console for children to inherit).
pub fn no_window(cmd: &mut Command) {
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        cmd.creation_flags(0x0800_0000);
    }
    #[cfg(not(windows))]
    let _ = cmd;
}

/// Resolve a program to (exe, prefix args). pnpm/yarn often aren't installed globally:
/// try corepack (bundled with Node ≤ 24), then `npx --yes` (Node 25+ dropped corepack),
/// so `pnpm install` still works as `npx --yes pnpm install`.
pub fn command_for(program: &str) -> Option<(PathBuf, Vec<String>)> {
    if let Some(p) = resolve(program) {
        return Some((p, vec![]));
    }
    if matches!(program, "pnpm" | "yarn") {
        if let Some(corepack) = resolve("corepack") {
            return Some((corepack, vec![program.to_string()]));
        }
        if let Some(npx) = resolve("npx") {
            return Some((npx, vec!["--yes".into(), program.to_string()]));
        }
    }
    if program == "python" {
        if let Some(py) = resolve("py") {
            return Some((py, vec!["-3".into()]));
        }
    }
    None
}
