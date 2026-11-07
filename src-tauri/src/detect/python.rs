//! Python apps packaged with PyInstaller. Order of preference: the project's own
//! `build_exe.py`, then a `.spec` file, then a generated `--onefile` build of the entry script.

use super::{Ctx, Detected, Note, Stack};
use crate::plan::{Step, StepKind, sv};

const ENTRY_CANDIDATES: &[&str] = &["main.py", "app.py", "__main__.py", "gui.py", "run.py"];
const GUI_HINTS: &[&str] = &["tkinter", "customtkinter", "PySide", "PyQt", "import wx", "pystray"];

pub fn detect(ctx: &Ctx) -> Option<Detected> {
    let has_reqs = ctx.has("requirements.txt");
    let has_pyproject = ctx.has("pyproject.toml");
    let specs = ctx.glob("*.spec");
    let py_files = ctx.glob("*.py");
    if !has_reqs && !has_pyproject && specs.is_empty() && py_files.is_empty() {
        return None;
    }
    // Loose .py files next to a package.json are usually helper scripts, not the app.
    let priority = if has_reqs || has_pyproject || !specs.is_empty() || ctx.has("build_exe.py") { 50 } else { 30 };

    let mut d = Detected::new(Stack::Python, priority);
    d.tags = vec!["Python".into()];

    let venv = [".venv", "venv"].into_iter().find(|v| ctx.has(&format!("{v}/pyvenv.cfg")));
    // Only create a venv when the project declares its deps; otherwise a fresh venv would be
    // empty and break builds that rely on packages installed globally (e.g. WindowsSizeScanner).
    let use_venv = venv.is_some() || has_reqs || has_pyproject;
    let venv_dir = venv.unwrap_or(".venv");
    let python = if use_venv {
        ctx.p(&format!("{venv_dir}/Scripts/python.exe")).to_string_lossy().into_owned()
    } else {
        "python".to_string()
    };

    if !use_venv {
        d.tools.push("python".into());
        d.notes.push(Note::info("No requirements.txt/pyproject.toml, so Faber uses your system Python as-is."));
    } else if !ctx.opts.skip_install {
        if venv.is_none() {
            d.tools.push("python".into());
            d.steps.push(
                Step::exec(StepKind::Install, "Create .venv", "python", sv(&["-m", "venv", ".venv"]))
                    .reason("no virtualenv yet, so Faber creates .venv to keep deps isolated"),
            );
        }
        let site = ctx.mtime(&format!("{venv_dir}/Lib/site-packages"));
        let stale = |rel: &str| venv.is_none() || ctx.opts.force_install || matches!((ctx.mtime(rel), site), (Some(f), Some(s)) if f > s);
        if has_reqs && stale("requirements.txt") {
            let why = if venv.is_none() { "fresh venv" } else { "requirements.txt changed since the last install" };
            d.steps.push(
                Step::exec(StepKind::Install, "pip install -r requirements.txt", &python, sv(&["-m", "pip", "install", "-r", "requirements.txt"]))
                    .reason(why),
            );
        } else if !has_reqs && has_pyproject && stale("pyproject.toml") {
            d.steps.push(
                Step::exec(StepKind::Install, "pip install .", &python, sv(&["-m", "pip", "install", "."]))
                    .reason("installing the project's declared dependencies"),
            );
        }
    }

    let needs_pyinstaller = !ctx.has("build_exe.py");
    if use_venv && needs_pyinstaller && !ctx.opts.skip_install && !ctx.has(&format!("{venv_dir}/Scripts/pyinstaller.exe")) {
        d.steps.push(
            Step::exec(StepKind::Install, "pip install pyinstaller", &python, sv(&["-m", "pip", "install", "pyinstaller"]))
                .reason("PyInstaller isn't installed in the venv"),
        );
    }

    if ctx.has("build_exe.py") {
        d.steps.push(Step::exec(StepKind::Build, "Run build_exe.py", &python, sv(&["build_exe.py"])));
    } else if let Some(spec) = specs.first() {
        let name = spec.file_name().unwrap_or_default().to_string_lossy().into_owned();
        d.steps.push(Step::exec(
            StepKind::Build,
            format!("PyInstaller {name}"),
            &python,
            vec!["-m".into(), "PyInstaller".into(), "--noconfirm".into(), name],
        ));
    } else if let Some(entry) = ENTRY_CANDIDATES
        .iter()
        .map(|e| e.to_string())
        .find(|e| ctx.has(e))
        // A lone script (e.g. Sticker Downloader's sigstick_dl.py) is the entry point by definition.
        .or_else(|| (py_files.len() == 1).then(|| py_files[0].file_name().unwrap_or_default().to_string_lossy().into_owned()))
    {
        let entry = entry.as_str();
        let mut args = sv(&["-m", "PyInstaller", "--noconfirm", "--onefile"]);
        if ctx.read(entry).is_some_and(|src| GUI_HINTS.iter().any(|h| src.contains(h))) {
            args.push("--windowed".into()); // GUI app: no console window behind it
        }
        if ctx.has("icon.ico") {
            args.extend(sv(&["--icon", "icon.ico"]));
        }
        args.push(entry.to_string());
        d.steps.push(Step::exec(StepKind::Build, format!("PyInstaller --onefile {entry}"), &python, args));
    } else {
        d.notes.push(Note::error("Couldn't find an entry point (main.py/app.py), a .spec, or build_exe.py."));
    }

    d.artifacts = sv(&["dist/*.exe", "dist/*/*.exe"]);
    d.clean = sv(&["build", "dist"]); // PyInstaller's work dir + output; .venv is deps, not build output
    Some(d)
}
