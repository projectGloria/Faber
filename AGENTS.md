# AGENTS.md — Faber

Faber is a Windows desktop build runner. Its React and TypeScript UI displays build plans, live logs, and artifacts from a Tauri 2 Rust backend.

## Commands

- Install frontend dependencies with `npm ci`. Launch the desktop app with `npm run tauri dev`.
- Typecheck and build the frontend with `npm run build`.
- Run the Rust suite with `cargo test --manifest-path src-tauri/Cargo.toml --locked`.
- Run the Cargo detector test with `cargo test --manifest-path src-tauri/Cargo.toml --locked cargo_crate`.
- After building the frontend, compile the release executable with `cargo build --manifest-path src-tauri/Cargo.toml --release --locked`.

## Architecture

- `src/App.tsx` hosts the build UI. `src/lib/api.ts` defines the frontend IPC interface and `src/hooks/useBuildRun.ts` handles build events.
- `src-tauri/src/commands.rs` accepts a project path and options. Build and clean each acquire the busy guard and repeat detection on disk before acting.
- `src-tauri/src/detect/` selects the stack and derives the plan. `src-tauri/src/plan.rs` defines the build steps and output patterns.
- `src-tauri/src/runner.rs` streams ordered events over a per-run Tauri channel and uses Windows Job Objects for process-tree cancellation.
- `src-tauri/src/clean.rs` validates detector-derived cleanup targets. Artifact actions in `commands.rs` reveal outputs in Explorer without executing them.
- **Version Manifests:** Keep `package.json`, `src-tauri/Cargo.toml`, `src-tauri/tauri.conf.json`, and `gloria.toml` in sync.

## Personal Rules (Brain)

- **Git Branching:** Work directly on `main`. Create a branch or worktree only on explicit request.
- **Commit Format:** Use one plain sentence explaining what changed and why. Omit conventional-commit prefixes.
- **Commit & Push Policy:** Commit, tag, or push only on explicit request. For “commit and push,” verify the build and tests, commit on `main`, and push to `origin main`. Never force-push to `main`.
- **Versioning:** Keep the current version during normal work. The user chooses the release version.
- **Releases:** Use `/monjita-github-release` to update all version manifests, dispatch `release.yml`, and verify publication. Never create release tags or releases by hand.
- **Release Workflow:** Use the shared GloriaSetup template with its check and release jobs, Faber’s own build job, and an unpinned GloriaSetup version.
- **Path Privacy:** Use `C:\Users\you` in documentation, comments, and examples.
- **Window Chrome:** Use a frameless titlebar with a drag region, double-click maximize, and custom minimize, maximize, and close controls.
- **Desktop Shell Lock:** Keep the shell at `100vw` by `100vh` with `overflow: hidden`, internal scroll panes, and `user-select: none` on chrome.
- **Scrollbars:** Use themed 4–6px scrollbars with transparent tracks and zero layout shift.
- **Interactive Controls:** Style form controls and their hover, active, focus-visible, and disabled states with 120–180ms transitions.

## Playbooks (Read Before Task)

- Read `.brain/building-the-project.md` before compiling release binaries, packaging Faber, or building an installer.
- Read `.brain/github-actions.md` before editing workflows, setting up release automation, or debugging a failed release.

## Repo Guardrails & Gotchas

- **Windows working directories:** Keep `dunce::canonicalize` in `src-tauri/src/commands.rs`. npm and cmd can reject extended-length Windows paths as working directories.
- **Release triggers:** The current `release.yml` can publish on a version-changing push to `main`, and `build-installers.yml` can publish on a `v*` tag. Account for both until the shared workflow is adopted.

## Done Criteria

Run `npm run build` and `cargo test --manifest-path src-tauri/Cargo.toml --locked`. Confirm the four version manifests agree.

<!-- brain-sync: 31e69a675214 -->
