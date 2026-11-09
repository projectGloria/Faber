//! Executes a BuildPlan and streams everything to the webview over a Tauri v2 `Channel`.
//! Channels are ordered and per-invocation, unlike global events, so output from one run
//! can never interleave with another listener.

use crate::detect::{Detection, glob_in};
use crate::plan::Action;
use crate::toolchain;
use serde::Serialize;
use std::path::{Path, PathBuf};
use std::process::Stdio;
use std::sync::Mutex;
use std::time::{Duration, Instant, SystemTime};
use tauri::ipc::Channel;
use tokio::io::{AsyncRead, AsyncReadExt};
use tokio::process::Command;

#[derive(Serialize, Clone)]
#[serde(tag = "type", rename_all = "camelCase", rename_all_fields = "camelCase")]
pub enum RunEvent {
    /// The plan actually being run (re-detected server-side, so it may differ from what the UI showed).
    Plan { detection: Detection },
    StepStarted { id: String, command: String },
    Output { id: String, data: String },
    StepFinished { id: String, code: Option<i32>, ms: u64, ok: bool },
    Artifacts { items: Vec<Artifact> },
    Done { ok: bool, cancelled: bool, ms: u64, error: Option<String> },
}

#[derive(Serialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct Artifact {
    pub path: String,
    pub name: String,
    pub size: u64,
    pub is_dir: bool,
}

#[derive(Default)]
struct Inner {
    running: bool,
    pid: Option<u32>,
    cancelled: bool,
}

#[derive(Default)]
pub struct RunState(Mutex<Inner>);

impl RunState {
    /// Kill the whole process tree. `child.kill()` only kills `npm.cmd`; the node/esbuild/
    /// cargo grandchildren would keep running and hold file locks on target/ or node_modules.
    pub fn cancel(&self) {
        let pid = {
            let mut g = self.0.lock().unwrap();
            if !g.running {
                return;
            }
            g.cancelled = true;
            g.pid.take()
        };
        if let Some(pid) = pid {
            let mut cmd = std::process::Command::new("taskkill");
            cmd.args(["/T", "/F", "/PID", &pid.to_string()]).stdout(Stdio::null()).stderr(Stdio::null());
            toolchain::no_window(&mut cmd);
            let _ = cmd.status();
        }
    }

    /// Claims the single "busy" slot. Builds and folder removal share it: deleting `target/`
    /// while cargo writes into it would fail half-way and leave a corrupt incremental cache.
    pub fn begin(&self) -> Result<RunGuard<'_>, String> {
        let mut g = self.0.lock().unwrap();
        if g.running {
            return Err("Faber is busy (a build or removal is already running).".into());
        }
        *g = Inner { running: true, ..Default::default() };
        Ok(RunGuard(self))
    }

    fn cancelled(&self) -> bool {
        self.0.lock().unwrap().cancelled
    }
}

/// Clears `running` even if the run future is dropped or errors out mid-way.
pub struct RunGuard<'a>(&'a RunState);
impl Drop for RunGuard<'_> {
    fn drop(&mut self) {
        let mut g = self.0.0.lock().unwrap();
        *g = Inner::default();
    }
}

#[allow(dead_code)]
pub async fn run(detection: Detection, state: &RunState, ch: Channel<RunEvent>) -> Result<(), String> {
    let guard = state.begin()?;
    run_with_guard(detection, state, guard, ch).await
}

pub async fn run_with_guard(detection: Detection, state: &RunState, _guard: RunGuard<'_>, ch: Channel<RunEvent>) -> Result<(), String> {
    let root = PathBuf::from(&detection.root);
    let plan = detection.plan.clone();
    let _ = ch.send(RunEvent::Plan { detection });

    // Slack for filesystems with coarse mtimes and tools that preserve source timestamps.
    let started_wall = SystemTime::now() - Duration::from_secs(2);
    let started = Instant::now();

    for step in &plan.steps {
        let _ = ch.send(RunEvent::StepStarted { id: step.id.clone(), command: step.command.clone() });
        let t0 = Instant::now();
        let Action::Exec { program, args, env, cwd } = &step.action;
        let result = exec(&root, &step.id, program, args, env, cwd.as_deref(), state, &ch).await;
        let ms = t0.elapsed().as_millis() as u64;
        let (ok, code, error) = match result {
            Ok(code) => (code == Some(0), code, None),
            Err(e) => (false, None, Some(e)),
        };
        let _ = ch.send(RunEvent::StepFinished { id: step.id.clone(), code, ms, ok });

        if !ok {
            let cancelled = state.cancelled();
            let error = error.or_else(|| (!cancelled).then(|| format!("`{}` exited with code {}", step.command, code.map_or("?".into(), |c| c.to_string()))));
            let _ = ch.send(RunEvent::Done { ok: false, cancelled, ms: started.elapsed().as_millis() as u64, error });
            return Ok(());
        }
    }

    let mut items = collect_artifacts(&root, &plan.artifacts, started_wall);
    // UNIX_EPOCH = "any mtime": pinned outputs are current by construction (see BuildPlan::pinned_artifacts).
    for p in collect_artifacts(&root, &plan.pinned_artifacts, SystemTime::UNIX_EPOCH) {
        if !items.iter().any(|i| i.path == p.path) {
            items.push(p);
        }
    }
    let _ = ch.send(RunEvent::Artifacts { items });
    let _ = ch.send(RunEvent::Done { ok: true, cancelled: false, ms: started.elapsed().as_millis() as u64, error: None });
    Ok(())
}

async fn exec(
    root: &Path,
    id: &str,
    program: &str,
    args: &[String],
    env: &[(String, String)],
    cwd: Option<&str>,
    state: &RunState,
    ch: &Channel<RunEvent>,
) -> Result<Option<i32>, String> {
    let (resolved, prefix) = toolchain::command_for(program).ok_or_else(|| format!("`{program}` not found on PATH"))?;
    let work_dir = match cwd {
        Some(rel) => root.join(rel),
        None => root.to_path_buf(),
    };
    let mut cmd = Command::new(&resolved);
    cmd.args(&prefix)
        .args(args)
        .current_dir(&work_dir)
        .envs(env.iter().map(|(k, v)| (k, v)))
        // Pipes aren't TTYs, so most tools would drop colour; these force it back on and xterm renders the ANSI.
        .env("FORCE_COLOR", "1")
        .env("CLICOLOR_FORCE", "1")
        .env("CARGO_TERM_COLOR", "always")
        .env("PY_COLORS", "1")
        .env("PYTHONUNBUFFERED", "1") // otherwise Python block-buffers stdout and output arrives in bursts
        .env("DOTNET_CLI_TELEMETRY_OPTOUT", "1")
        .stdin(Stdio::null()) // nothing can prompt; a hung "Ok to proceed? (y)" would stall forever
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .kill_on_drop(true);
    #[cfg(windows)]
    cmd.creation_flags(0x0800_0000); // CREATE_NO_WINDOW

    let mut child = cmd.spawn().map_err(|e| format!("Failed to start `{program}`: {e}"))?;
    {
        let mut g = state.0.lock().unwrap();
        if g.cancelled {
            // Cancel landed between steps: don't let the next step run.
            drop(g);
            let _ = child.start_kill();
            return Ok(None);
        }
        g.pid = child.id();
    }

    let out = tokio::spawn(pump(child.stdout.take().unwrap(), id.to_string(), ch.clone()));
    let err = tokio::spawn(pump(child.stderr.take().unwrap(), id.to_string(), ch.clone()));
    let status = child.wait().await.map_err(|e| e.to_string())?;
    state.0.lock().unwrap().pid = None;

    // A daemon the build spawned (turbo, gradle...) can inherit our pipe handles and keep them open
    // forever after the main process exits, so give the readers a short grace period then move on.
    let abort_out = out.abort_handle();
    let abort_err = err.abort_handle();
    let readers = async {
        let _ = tokio::join!(out, err);
    };
    if tokio::time::timeout(Duration::from_secs(2), readers).await.is_err() {
        abort_out.abort();
        abort_err.abort();
    }
    Ok(status.code())
}

/// Forward raw chunks (not lines): progress bars redraw with `\r` and never emit `\n`, so
/// line-buffering would freeze them. The carry buffer keeps multi-byte UTF-8 chars split
/// across reads from turning into U+FFFD.
async fn pump<R: AsyncRead + Unpin>(mut r: R, id: String, ch: Channel<RunEvent>) {
    let mut buf = vec![0u8; 16 * 1024];
    let mut carry: Vec<u8> = Vec::new();
    loop {
        let n = match r.read(&mut buf).await {
            Ok(0) | Err(_) => break,
            Ok(n) => n,
        };
        carry.extend_from_slice(&buf[..n]);
        let cut = match std::str::from_utf8(&carry) {
            Ok(_) => carry.len(),
            Err(e) if e.error_len().is_none() => e.valid_up_to(), // truncated char at the end: wait for the rest
            Err(_) => carry.len(), // genuinely invalid (OEM codepage output): flush lossy
        };
        if cut > 0 {
            let data = String::from_utf8_lossy(&carry[..cut]).into_owned();
            carry.drain(..cut);
            let _ = ch.send(RunEvent::Output { id: id.clone(), data });
        }
    }
    if !carry.is_empty() {
        let _ = ch.send(RunEvent::Output { id, data: String::from_utf8_lossy(&carry).into_owned() });
    }
}

fn collect_artifacts(root: &Path, patterns: &[String], since: SystemTime) -> Vec<Artifact> {
    let mut seen = std::collections::HashSet::new();
    let mut items = vec![];
    for pat in patterns {
        for p in glob_in(root, pat) {
            let fresh = crate::detect::mtime(&p).is_some_and(|m| m >= since);
            if !fresh || !seen.insert(p.clone()) {
                continue;
            }
            let is_dir = p.is_dir();
            let size = if is_dir { dir_size(&p, &mut 0) } else { std::fs::metadata(&p).map(|m| m.len()).unwrap_or(0) };
            items.push(Artifact {
                name: p.file_name().unwrap_or_default().to_string_lossy().into_owned(),
                path: dunce::simplified(&p).to_string_lossy().into_owned(),
                size,
                is_dir,
            });
        }
    }
    items
}

/// Recursive size with an entry budget, so a 200k-file output dir can't stall the UI.
fn dir_size(p: &Path, budget: &mut u32) -> u64 {
    let Ok(rd) = std::fs::read_dir(p) else { return 0 };
    let mut total = 0;
    for e in rd.flatten() {
        *budget += 1;
        if *budget > 20_000 {
            break;
        }
        match e.file_type() {
            Ok(t) if t.is_dir() && !t.is_symlink() => total += dir_size(&e.path(), budget),
            Ok(_) => total += e.metadata().map(|m| m.len()).unwrap_or(0),
            Err(_) => {}
        }
    }
    total
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::detect::{Options, detect};
    use std::sync::{Arc, Mutex as StdMutex};
    use tauri::ipc::InvokeResponseBody;

    fn capture() -> (Channel<RunEvent>, Arc<StdMutex<Vec<serde_json::Value>>>) {
        let events = Arc::new(StdMutex::new(Vec::new()));
        let sink = events.clone();
        let ch = Channel::new(move |body| {
            if let InvokeResponseBody::Json(s) = body {
                sink.lock().unwrap().push(serde_json::from_str(&s).unwrap());
            }
            Ok(())
        });
        (ch, events)
    }

    fn types(ev: &[serde_json::Value]) -> Vec<String> {
        ev.iter().map(|e| e["type"].as_str().unwrap().to_string()).collect()
    }

    #[tokio::test]
    async fn runs_script_streams_output_and_collects_fresh_artifacts() {
        let t = tempfile::tempdir().unwrap();
        std::fs::write(t.path().join("build.bat"), "@echo off\r\necho hello from build\r\necho fake> App.exe\r\n").unwrap();
        let d = detect(t.path(), Options::default());
        let state = RunState::default();
        let (ch, events) = capture();
        run(d, &state, ch).await.unwrap();

        let ev = events.lock().unwrap();
        let ty = types(&ev);
        assert_eq!(ty.first().unwrap(), "plan");
        assert!(ty.contains(&"output".to_string()));
        let out: String = ev.iter().filter(|e| e["type"] == "output").map(|e| e["data"].as_str().unwrap()).collect();
        assert!(out.contains("hello from build"), "output was {out:?}");
        let arts = ev.iter().find(|e| e["type"] == "artifacts").unwrap();
        assert_eq!(arts["items"][0]["name"], "App.exe");
        let done = ev.last().unwrap();
        assert_eq!(done["type"], "done");
        assert_eq!(done["ok"], true);
        assert!(!state.0.lock().unwrap().running, "guard must clear running flag");
    }

    #[tokio::test]
    async fn failing_step_stops_the_plan() {
        let t = tempfile::tempdir().unwrap();
        std::fs::write(t.path().join("build.bat"), "@echo off\r\nexit /b 3\r\n").unwrap();
        let (ch, events) = capture();
        run(detect(t.path(), Options::default()), &RunState::default(), ch).await.unwrap();
        let ev = events.lock().unwrap();
        let fin = ev.iter().find(|e| e["type"] == "stepFinished").unwrap();
        assert_eq!(fin["code"], 3);
        assert_eq!(ev.last().unwrap()["ok"], false);
        assert!(ev.last().unwrap()["error"].as_str().unwrap().contains("exited with code 3"));
    }

    #[tokio::test]
    async fn cancel_kills_the_process_tree() {
        let t = tempfile::tempdir().unwrap();
        // cmd → ping grandchild: exactly the npm → node shape that a plain child.kill() would orphan.
        std::fs::write(t.path().join("build.bat"), "@echo off\r\nping -n 60 127.0.0.1 > nul\r\n").unwrap();
        let state = Arc::new(RunState::default());
        let (ch, events) = capture();
        let s2 = state.clone();
        let canceller = tokio::spawn(async move {
            tokio::time::sleep(Duration::from_millis(1500)).await;
            s2.cancel();
        });
        let t0 = Instant::now();
        run(detect(t.path(), Options::default()), &state, ch).await.unwrap();
        canceller.await.unwrap();
        assert!(t0.elapsed() < Duration::from_secs(15), "cancel didn't stop the run");
        let done = events.lock().unwrap().last().unwrap().clone();
        assert_eq!(done["cancelled"], true);
        assert_eq!(done["ok"], false);
    }
}
