# Faber

Pick a project folder (or drop it on the window). Faber detects the stack, installs what's missing (and says why), builds it, streams the output into a terminal, and lists the files it produced.

```bash
npm i
npm run tauri dev     # develop
npm run tauri build   # → src-tauri/target/release/bundle/{nsis,msi}
```

## What it detects

| Stack | Trigger | Runs |
|---|---|---|
| Tauri | `src-tauri/tauri.conf.json`, `app/src-tauri/...` | `<pm> tauri build` (+ `--bundles nsis,msi` unless the config pins targets); portable build is the native release exe in `target/release/` |
| Electron | `electron` + `electron-builder` | your `dist`/`build:win`/`package` script, with `nsis portable` appended when safe; otherwise `electron-builder --win nsis portable` |
| Flutter | `pubspec.yaml` with `flutter:` | `flutter pub get` (if deps stale/missing) → `flutter build windows --release` (or `apk --release` / `web`) |
| Dart | `pubspec.yaml` without flutter | `dart pub get` → `dart compile exe bin/<entry>.dart -o bin/<name>.exe` |
| Gradle | `build.gradle(.kts)`, `settings.gradle(.kts)`, `gradlew.bat` | `cmd /C .\gradlew.bat assembleRelease` (Android APK) or `build` (Java/Kotlin) |
| Script | `build.ps1`, `Build-*.ps1`, `build-*.bat`, `build-*.cmd`, `installer/build.ps1`, etc. | your custom script; outranks generic rules |
| .NET | SDK-style `*.csproj` / `*.sln` | `dotnet publish -r win-x64 -p:PublishSingleFile=true` → `bin/Release/<tfm>/win-x64/publish/` |
| VS / MSBuild | `*.vcxproj`, legacy csproj | MSBuild (found via vswhere), Release x64 |
| Cargo | `Cargo.toml` | `cargo build --release` |
| Zig | `build.zig` | `zig build -Doptimize=ReleaseSafe` → `zig-out/bin/*.exe` |
| Go | `go.mod`, `main.go` | `go mod download` → `go build -ldflags "-s -w" -o <name>.exe .` |
| CMake / Make | `CMakeLists.txt`, `Makefile` | `cmake -B build` + `cmake --build build --config Release` (or `make`) |
| Extension | VS Code (`engines.vscode`) / WebExtension (`manifest.json`) | `<pm> run package` or `@vscode/vsce package` (`*.vsix`), or zip extension packaging (`*.zip`) |
| Python | `requirements.txt` / `.spec` / `build_exe.py` / entry `.py` | `.venv` + pip only when deps are declared; `build_exe.py` > `.spec` > `pyinstaller --onefile` |
| Node | `package.json` | `<pm> run build` (WXT also runs `zip`) |

**Installs** run only when needed: `node_modules` missing, or the lockfile/`package.json` is newer than the package manager's install marker. If `pnpm`/`yarn` aren't installed, Faber runs them through corepack or `npx --yes`.

**Per-project settings** (portable/installer toggles, skip/force install, extra args) are stored per folder in the app's `faber.json` store.

**Artifacts** only list files modified during the run, so stale builds never show up. Everything stays in each toolchain's native output folder; Faber never copies into a folder of its own.

**Remove uninstallers** deletes the detected build output folders after a confirm dialog: `target/` (Cargo/Tauri), `dist`/`out`/`.output` (JS), the electron-builder output dir, `bin`+`obj` (.NET/MSBuild), `build`+`dist` (PyInstaller), Inno's `Output/`, and leftover `faber-out/` from older versions. Dependency folders (`node_modules`, `.venv`), `.git`, symlinks/junctions and anything outside the project root are refused in Rust, whatever the detector asks for.

## Tests

```bash
cd src-tauri
cargo test                                         # detectors + runner (spawn, stream, fail, cancel tree-kill)
cargo test real_projects -- --ignored --nocapture  # dry-run detection over every sibling project
```
