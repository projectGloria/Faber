import { useEffect, useEffectEvent, useMemo, useRef, useState } from "react";
import { ask, open } from "@tauri-apps/plugin-dialog";
import { getCurrentWebview } from "@tauri-apps/api/webview";
import { DEFAULT_OPTIONS, detectProject, type Detection, type Options } from "./lib/api";
import { basename, formatMs } from "./lib/format";
import { usePrefs } from "./hooks/usePrefs";
import { useBuildRun } from "./hooks/useBuildRun";
import { TitleBar } from "./components/TitleBar";
import { Terminal, type TerminalHandle } from "./components/Terminal";
import { PlanCard } from "./components/PlanCard";
import { Artifacts } from "./components/Artifacts";
import { EmptyState } from "./components/EmptyState";

export default function App() {
  const prefs = usePrefs();
  const [path, setPath] = useState<string | null>(null);
  const [detection, setDetection] = useState<Detection | null>(null);
  const [detectError, setDetectError] = useState<string | null>(null);
  const [dragging, setDragging] = useState(false);
  const termRef = useRef<TerminalHandle>(null);
  const { run, start, cancel, clean, reset } = useBuildRun(termRef);
  // Derived, not stored: building and deleting folders both lock the project, one source of truth.
  const running = run.phase === "running" || run.phase === "cleaning";

  // Memoized on `optionsFor` (which only changes with overrides) so the detect effect below gets a
  // stable dependency; a fresh object every render would re-run detection in an endless loop.
  const options = useMemo(() => (path ? prefs.optionsFor(path) : DEFAULT_OPTIONS), [path, prefs.optionsFor]);

  // Detection reads the filesystem, an external system, so an effect is the right tool here.
  // `completedRuns` re-detects after each build: install reasons disappear once deps are in.
  useEffect(() => {
    if (!path || !prefs.loaded) return;
    let alive = true; // drop responses from a superseded path/options combo (out-of-order resolves)
    detectProject(path, options)
      .then((d) => {
        if (!alive) return;
        setDetection(d);
        setDetectError(null);
        if (d.root && d.root !== path) {
          setPath(d.root);
        }
        prefs.pushRecent(d.root || path);
      })
      .catch((e) => alive && setDetectError(String(e)));
    return () => {
      alive = false;
    };
  }, [path, options, prefs.loaded, run.completedRuns]);

  function openProject(p: string) {
    if (running) return;
    // Reset in the event handler, not in an effect keyed on `path`: no stale frame where the old
    // project's plan renders under the new project's name.
    setPath(p);
    setDetection(null);
    setDetectError(null);
    reset();
    termRef.current?.clear();
  }

  async function pickFolder() {
    const picked = await open({ directory: true, multiple: false, defaultPath: path ?? undefined });
    if (typeof picked === "string") openProject(picked);
  }

  function changeOptions(o: Options) {
    if (!path) return;
    reset(); // the previous run's statuses don't describe the new plan
    prefs.setOptionsFor(path, o);
  }

  async function removeBuildFolders(folders: string[]) {
    if (!path || folders.length === 0) return;
    // Irreversible and potentially GBs (a full cargo target/), so it always asks first.
    const ok = await ask(`Permanently delete these folders in ${basename(path)}?\n\n${folders.join("\n")}\n\nThe next build starts from scratch.`, {
      title: "Remove uninstallers",
      kind: "warning",
      okLabel: "Delete",
      cancelLabel: "Keep",
    });
    if (ok) await clean(path, options);
  }

  // useEffectEvent (React 19.2+): the drag-drop listener is registered once, but always calls the
  // *latest* openProject, so it sees the current `running` value and not the one from mount time.
  const onDrop = useEffectEvent((p: string) => openProject(p));
  useEffect(() => {
    let unlisten: (() => void) | undefined;
    let disposed = false;
    getCurrentWebview()
      .onDragDropEvent((e) => {
        const t = e.payload.type;
        if (t === "enter" || t === "over") setDragging(true);
        else if (t === "leave") setDragging(false);
        else if (t === "drop") {
          setDragging(false);
          if (e.payload.paths[0]) onDrop(e.payload.paths[0]);
        }
      })
      .then((fn) => (disposed ? fn() : (unlisten = fn))); // StrictMode: cleanup may run before the promise resolves
    return () => {
      disposed = true;
      unlisten?.();
    };
  }, []);

  // While a run is active (or just finished) show the plan Rust actually executed so step ids
  // line up with the streamed statuses; otherwise show the live detection.
  const shown = run.phase !== "idle" && run.plan ? run.plan : detection;

  if (!path) {
    return (
      <div className="app">
        <TitleBar />
        <EmptyState recents={prefs.recents} onPick={pickFolder} onOpen={openProject} onForget={prefs.removeRecent} dragging={dragging} />
      </div>
    );
  }

  return (
    <div className={`app ${dragging ? "dragging" : ""}`}>
      <TitleBar>
        <div className="project-title" data-tauri-drag-region>
          <span className="project-name">{shown?.name ?? basename(path)}</span>
          <span className="project-path muted" data-tauri-drag-region>{path}</span>
        </div>
        <button className="btn btn-ghost small" onClick={pickFolder} disabled={running}>Change</button>
        <button className="btn btn-ghost small" onClick={() => setPath(null)} disabled={running} aria-label="Close project">×</button>
      </TitleBar>

      <main className="workspace">
        <aside className="side">
          {detectError && <div className="card error-card">{detectError}</div>}
          {shown ? (
            <PlanCard
              detection={shown}
              // Always the live detection's list: `shown` may be the last run's snapshot, taken before
              // the build created (or a clean removed) those folders.
              cleanable={detection?.cleanable ?? []}
              options={options}
              onOptions={changeOptions}
              run={run}
              onBuild={() => start(path, options)}
              onCancel={cancel}
              onClean={removeBuildFolders}
            />
          ) : (
            !detectError && <div className="card plan skeleton">Detecting…</div>
          )}
          <Artifacts items={run.artifacts} />
        </aside>

        <section className="term-pane">
          <div className="term-head">
            <span>Output</span>
            <RunStatus run={run} />
            <button className="btn btn-ghost small" onClick={() => termRef.current?.clear()} disabled={running}>Clear</button>
          </div>
          <Terminal ref={termRef} />
        </section>
      </main>
    </div>
  );
}

function RunStatus({ run }: { run: ReturnType<typeof useBuildRun>["run"] }) {
  if (run.phase === "running") return <span className="run-status running"><span className="spinner" /> Building…</span>;
  if (run.phase === "cleaning") return <span className="run-status running"><span className="spinner" /> Removing…</span>;
  if (!run.result) return <span className="run-status" />;
  const r = run.result;
  if (r.ok) return <span className="run-status ok">✓ Built in {formatMs(r.ms)}</span>;
  if (r.cancelled) return <span className="run-status warn">Cancelled</span>;
  return <span className="run-status fail">✗ Failed</span>;
}
