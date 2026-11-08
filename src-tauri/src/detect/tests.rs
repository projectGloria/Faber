//! Detector tests against throwaway project folders shaped like the real repos.

use super::*;
use std::fs;
use std::path::Path;

fn write(root: &Path, rel: &str, body: &str) {
    let p = root.join(rel);
    fs::create_dir_all(p.parent().unwrap()).unwrap();
    fs::write(p, body).unwrap();
}

fn commands(d: &Detection) -> Vec<String> {
    d.plan.steps.iter().map(|s| s.command.clone()).collect()
}

fn opts() -> Options {
    Options::default()
}

#[test]
fn tauri_with_missing_node_modules() {
    let t = tempfile::tempdir().unwrap();
    write(t.path(), "package.json", r#"{"version":"1.2.0","scripts":{"tauri":"tauri"},"devDependencies":{"@tauri-apps/cli":"^2","vite":"^8","react":"^19"}}"#);
    write(t.path(), "package-lock.json", "{}");
    write(t.path(), "src-tauri/tauri.conf.json", r#"{"productName":"My App","version":"1.2.0"}"#);
    write(t.path(), "src-tauri/Cargo.toml", "[package]\nname = \"my-app\"\nversion = \"1.2.0\"\n");

    let d = detect(t.path(), opts());
    assert_eq!(d.stack, Some(Stack::Tauri));
    assert_eq!(d.package_manager, Some(PackageManager::Npm));
    assert!(d.tags.contains(&"React".to_string()));
    assert_eq!(commands(&d)[0], "npm i");
    assert!(d.plan.steps[0].reason.as_deref().unwrap().contains("node_modules is missing"));
    assert_eq!(commands(&d)[1], "npm run tauri -- build --bundles nsis,msi");
    // Portable = the native release exe, no copy step and no faber-out/.
    assert_eq!(d.plan.steps.len(), 2);
    assert_eq!(d.plan.pinned_artifacts, vec!["src-tauri/target/release/my-app.exe"]);
    assert!(d.plan.clean.iter().any(|c| c == "src-tauri/target"));
}

#[test]
fn tauri_respects_pinned_targets_and_workspace_target_dir() {
    let t = tempfile::tempdir().unwrap();
    write(t.path(), "package.json", r#"{"devDependencies":{"@tauri-apps/cli":"^2"}}"#);
    write(t.path(), "node_modules/.package-lock.json", "{}");
    write(t.path(), "Cargo.toml", "[workspace]\nmembers = [\"src-tauri\"]\n");
    write(t.path(), "src-tauri/tauri.conf.json", r#"{"productName":"Omni","version":"0.1.0","bundle":{"targets":["nsis"]}}"#);

    let d = detect(t.path(), opts());
    assert_eq!(commands(&d)[0], "npx tauri build"); // no `tauri` script, pinned targets → no --bundles
    assert!(d.plan.artifacts.iter().any(|a| a == "target/release/bundle/nsis/*.exe"));
    assert!(d.plan.clean.iter().any(|c| c == "target"));
}

#[test]
fn cleanable_lists_existing_build_folders_only() {
    let t = tempfile::tempdir().unwrap();
    write(t.path(), "package.json", r#"{"devDependencies":{"@tauri-apps/cli":"^2"}}"#);
    write(t.path(), "node_modules/.package-lock.json", "{}");
    write(t.path(), "src-tauri/tauri.conf.json", r#"{"productName":"X","build":{"frontendDist":"../web-dist"}}"#);
    write(t.path(), "src-tauri/target/release/x.exe", "");
    write(t.path(), "web-dist/index.html", "");
    write(t.path(), "faber-out/old-portable.exe", "");
    let d = detect(t.path(), opts());
    assert_eq!(d.cleanable, vec!["src-tauri/target", "web-dist", "faber-out"]);
}

#[test]
fn tauri_portable_only_skips_bundling() {
    let t = tempfile::tempdir().unwrap();
    write(t.path(), "package.json", r#"{"scripts":{"tauri":"tauri"},"devDependencies":{"@tauri-apps/cli":"^2"}}"#);
    write(t.path(), "node_modules/.package-lock.json", "{}");
    write(t.path(), "src-tauri/tauri.conf.json", r#"{"productName":"X"}"#);
    let d = detect(t.path(), Options { installer: false, ..opts() });
    assert_eq!(commands(&d)[0], "npm run tauri -- build --no-bundle");
}

#[test]
fn electron_appends_targets_after_trailing_win() {
    let t = tempfile::tempdir().unwrap();
    write(t.path(), "package.json", r#"{"scripts":{"build":"electron-vite build","dist":"npm run build && electron-builder --win"},"devDependencies":{"electron":"1","electron-builder":"1","electron-vite":"1"}}"#);
    write(t.path(), "node_modules/.package-lock.json", "{}");
    write(t.path(), "electron-builder.yml", "appId: x\ndirectories:\n  output: release\n  buildResources: build\n");
    let d = detect(t.path(), opts());
    assert_eq!(d.stack, Some(Stack::Electron));
    assert_eq!(commands(&d), vec!["npm run dist -- nsis portable"]);
    assert_eq!(d.plan.artifacts[0], "release/*.exe");
}

#[test]
fn electron_pinned_or_chained_script_runs_unchanged() {
    let t = tempfile::tempdir().unwrap();
    // Hedwig-style pinned targets.
    write(t.path(), "package.json", r#"{"scripts":{"dist":"npm run build && electron-builder --win nsis --x64"},"devDependencies":{"electron":"1","electron-builder":"1"}}"#);
    write(t.path(), "node_modules/.package-lock.json", "{}");
    assert_eq!(commands(&detect(t.path(), opts())), vec!["npm run dist"]);

    // Draco-style: something runs after electron-builder.
    write(t.path(), "package.json", r#"{"scripts":{"pack":"electron-builder --dir","dist":"electron-builder --win && npm run cli"},"devDependencies":{"electron":"1","electron-builder":"1"}}"#);
    assert_eq!(commands(&detect(t.path(), Options { skip_install: true, ..opts() })), vec!["npm run dist"]);
}

#[test]
fn electron_without_script_calls_builder_directly() {
    let t = tempfile::tempdir().unwrap();
    write(t.path(), "package.json", r#"{"scripts":{"build":"vite build"},"devDependencies":{"electron":"1","electron-builder":"1"}}"#);
    write(t.path(), "pnpm-lock.yaml", "");
    let d = detect(t.path(), Options { skip_install: true, ..opts() });
    assert_eq!(d.package_manager, Some(PackageManager::Pnpm));
    assert_eq!(commands(&d), vec!["pnpm run build", "pnpm exec electron-builder --win nsis portable"]);
}

#[test]
fn package_manager_field_wins_and_stale_lockfile_triggers_install() {
    let t = tempfile::tempdir().unwrap();
    write(t.path(), "node_modules/.modules.yaml", "");
    std::thread::sleep(std::time::Duration::from_millis(50));
    write(t.path(), "package.json", r#"{"packageManager":"pnpm@11.0.0","scripts":{"build":"turbo run build"},"devDependencies":{"turbo":"2"}}"#);
    write(t.path(), "pnpm-lock.yaml", "");
    let d = detect(t.path(), opts());
    assert_eq!(d.stack, Some(Stack::Node));
    assert_eq!(commands(&d), vec!["pnpm install", "pnpm run build"]);
    assert!(d.plan.steps[0].reason.as_deref().unwrap().contains("pnpm-lock.yaml changed"));
}

#[test]
fn fresh_node_modules_skips_install() {
    let t = tempfile::tempdir().unwrap();
    write(t.path(), "package.json", r#"{"scripts":{"build":"vite build"}}"#);
    write(t.path(), "package-lock.json", "{}");
    std::thread::sleep(std::time::Duration::from_millis(50));
    write(t.path(), "node_modules/.package-lock.json", "{}");
    assert_eq!(commands(&detect(t.path(), opts())), vec!["npm run build"]);
}

#[test]
fn cargo_crate() {
    let t = tempfile::tempdir().unwrap();
    write(t.path(), "Cargo.toml", "[package]\nname = \"nyx\"\n");
    write(t.path(), "install.ps1", "");
    let d = detect(t.path(), opts());
    assert_eq!(d.stack, Some(Stack::Cargo));
    assert_eq!(commands(&d), vec!["cargo build --release"]);
}

#[test]
fn dotnet_sdk_project() {
    let t = tempfile::tempdir().unwrap();
    write(t.path(), "PocketTray.csproj", "<Project Sdk=\"Microsoft.NET.Sdk\"></Project>");
    let d = detect(t.path(), opts());
    assert_eq!(d.stack, Some(Stack::Dotnet));
    let c = commands(&d);
    assert_eq!(c[0], "dotnet restore PocketTray.csproj");
    assert!(c[1].starts_with("dotnet publish PocketTray.csproj -c Release -r win-x64"));
    assert!(!c[1].contains(" -o "), "publish must use the SDK's native bin/Release/.../publish layout");
    assert_eq!(d.plan.artifacts[0], "bin/Release/*/win-x64/publish/*.exe");
}

#[test]
fn custom_script_beats_python() {
    let t = tempfile::tempdir().unwrap();
    write(t.path(), "app.py", "import tkinter");
    write(t.path(), "build.ps1", "");
    let d = detect(t.path(), opts());
    assert_eq!(d.stack, Some(Stack::Script));
    assert!(commands(&d)[0].starts_with("powershell -NoProfile -ExecutionPolicy Bypass -File"));
    assert_eq!(d.alternatives, vec![Stack::Python]);
}

#[test]
fn python_build_exe_uses_system_python_without_requirements() {
    let t = tempfile::tempdir().unwrap();
    write(t.path(), "main.py", "");
    write(t.path(), "build_exe.py", "");
    write(t.path(), "SizeScanner.spec", "");
    let d = detect(t.path(), opts());
    assert_eq!(d.stack, Some(Stack::Python));
    assert_eq!(commands(&d), vec!["python build_exe.py"]);
}

#[test]
fn python_requirements_and_spec_get_venv() {
    let t = tempfile::tempdir().unwrap();
    write(t.path(), "requirements.txt", "requests\n");
    write(t.path(), "KH.spec", "");
    write(t.path(), "kh.py", "");
    let c = commands(&detect(t.path(), opts()));
    assert_eq!(c[0], "python -m venv .venv");
    assert_eq!(c[1], "python.exe -m pip install -r requirements.txt");
    assert_eq!(c[2], "python.exe -m pip install pyinstaller");
    assert_eq!(c[3], "python.exe -m PyInstaller --noconfirm KH.spec");
}

#[test]
fn python_gui_entry_gets_windowed_onefile() {
    let t = tempfile::tempdir().unwrap();
    write(t.path(), "requirements.txt", "");
    std::thread::sleep(std::time::Duration::from_millis(50));
    write(t.path(), ".venv/pyvenv.cfg", "");
    write(t.path(), ".venv/Lib/site-packages/x", "");
    write(t.path(), ".venv/Scripts/pyinstaller.exe", "");
    write(t.path(), "app.py", "import customtkinter");
    let c = commands(&detect(t.path(), opts()));
    assert_eq!(c, vec!["python.exe -m PyInstaller --noconfirm --onefile --windowed app.py"]);
}

#[test]
fn extra_args_go_to_last_build_step() {
    let t = tempfile::tempdir().unwrap();
    write(t.path(), "Cargo.toml", "[package]\nname = \"x\"\n");
    let d = detect(t.path(), Options { extra_args: "--features fast".into(), ..opts() });
    assert_eq!(commands(&d), vec!["cargo build --release --features fast"]);
}

#[test]
fn extra_args_preserve_quoted_strings() {
    let t = tempfile::tempdir().unwrap();
    write(t.path(), "Cargo.toml", "[package]\nname = \"x\"\n");
    let d = detect(t.path(), Options { extra_args: "--features \"fast simd\" --target x86_64".into(), ..opts() });
    assert_eq!(commands(&d), vec!["cargo build --release --features \"fast simd\" --target x86_64"]);
}

#[test]
fn cargo_package_name_handles_comments_quotes_and_prefix_keys() {
    let toml = r#"
[package]
namespace = "my-scope"
name = "my-crate" # inline comment
version = "0.1.0"
"#;
    assert_eq!(cargo::package_name(toml), Some("my-crate".into()));

    let toml_single = "[package]\nname = 'single-quoted'\n";
    assert_eq!(cargo::package_name(toml_single), Some("single-quoted".into()));
}

#[test]
fn dotnet_subfolder_project_keeps_relative_path() {
    let t = tempfile::tempdir().unwrap();
    write(t.path(), "src/App/App.csproj", "<Project Sdk=\"Microsoft.NET.Sdk\"></Project>");
    let d = detect(t.path(), opts());
    assert_eq!(d.stack, Some(Stack::Dotnet));
    let c = commands(&d);
    assert_eq!(c[0], "dotnet restore src/App/App.csproj");
    assert!(c[1].starts_with("dotnet publish src/App/App.csproj -c Release"));
}

#[test]
fn bun_lockb_triggers_install_on_change() {
    let t = tempfile::tempdir().unwrap();
    write(t.path(), "node_modules", "");
    std::thread::sleep(std::time::Duration::from_millis(50));
    write(t.path(), "package.json", r#"{"scripts":{"build":"bun run src/index.ts"}}"#);
    write(t.path(), "bun.lockb", "binary-lock-data");
    let d = detect(t.path(), opts());
    assert_eq!(d.package_manager, Some(PackageManager::Bun));
    assert_eq!(commands(&d)[0], "bun install");
    assert!(d.plan.steps[0].reason.as_deref().unwrap().contains("bun.lockb changed"));
}

#[test]
fn empty_folder_reports_error() {
    let t = tempfile::tempdir().unwrap();
    let d = detect(t.path(), opts());
    assert_eq!(d.stack, None);
    assert_eq!(d.notes[0].level, Level::Error);
}

/// Dry run over the real projects next to Faber: `cargo test real_projects -- --ignored --nocapture`.
#[test]
#[ignore]
fn real_projects() {
    let workspace = Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
    let mut dirs: Vec<_> = fs::read_dir(workspace).unwrap().flatten().filter(|e| e.path().is_dir()).collect();
    dirs.sort_by_key(|e| e.file_name());
    for e in dirs {
        let d = detect(&e.path(), opts());
        println!("\n== {} [{:?} {:?}] tags={:?} alt={:?}", d.name, d.stack, d.package_manager, d.tags, d.alternatives);
        for s in &d.plan.steps {
            println!("   {:?} {}{}", s.kind, s.command, s.reason.as_ref().map(|r| format!("   ({r})")).unwrap_or_default());
        }
        for n in &d.notes {
            println!("   [{:?}] {}", n.level, n.text);
        }
        if !d.cleanable.is_empty() {
            println!("   clean: {}", d.cleanable.join(", "));
        }
    }
}
