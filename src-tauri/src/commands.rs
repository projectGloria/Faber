use crate::clean::{self, CleanReport};
use crate::detect::{self, Detection, Options};
use crate::runner::{self, RunEvent, RunState};
use std::path::PathBuf;
use tauri::State;
use tauri::ipc::Channel;

fn root(path: &str) -> Result<PathBuf, String> {
    let p = PathBuf::from(path);
    if !p.is_dir() {
        return Err(format!("Not a folder: {path}"));
    }
    // dunce: canonical path without the `\?\` prefix, which npm/cmd choke on as a cwd.
    dunce::canonicalize(&p).map_err(|e| e.to_string())
}

/// `async` so the filesystem scan (and vswhere for MSBuild) runs off the main thread;
/// sync Tauri commands block the event loop and freeze the window.
#[tauri::command]
pub async fn detect_project(path: String, options: Options) -> Result<Detection, String> {
    let root = root(&path)?;
    Ok(tauri::async_runtime::spawn_blocking(move || detect::detect(&root, options)).await.map_err(|e| e.to_string())?)
}

/// Takes path + options, not a plan: the webview never hands Rust an arbitrary command line.
/// The plan is re-detected here so it reflects the folder's state at the moment you click Build.
#[tauri::command]
pub async fn run_build(path: String, options: Options, state: State<'_, RunState>, on_event: Channel<RunEvent>) -> Result<(), String> {
    let root = root(&path)?;
    // Grab the busy guard up front: a Build click while already running is rejected before scanning.
    let _busy = state.begin()?;
    let detection = tauri::async_runtime::spawn_blocking(move || detect::detect(&root, options)).await.map_err(|e| e.to_string())?;
    if detection.plan.steps.is_empty() {
        return Err("Nothing to build here.".into());
    }
    runner::run_with_guard(detection, &state, _busy, on_event).await
}

/// async: taskkill takes ~100ms and sync commands run on the main thread.
#[tauri::command]
pub async fn cancel_build(state: State<'_, RunState>) -> Result<(), String> {
    state.cancel();
    Ok(())
}

/// "Remove uninstallers": deletes the plan's build folders. Same contract as `run_build`: the
/// webview sends a project path, never a folder list, so it can't be used to delete arbitrary
/// paths; the list is re-derived from the detector at click time.
#[tauri::command]
pub async fn clean_project(path: String, options: Options, state: State<'_, RunState>) -> Result<CleanReport, String> {
    let root = root(&path)?;
    // Held across the await: a Build click during a multi-GB `target/` delete gets rejected.
    let _busy = state.begin()?;
    tauri::async_runtime::spawn_blocking(move || {
        let patterns = detect::detect(&root, options).plan.clean;
        clean::remove(&root, &patterns)
    })
    .await
    .map_err(|e| e.to_string())
}
