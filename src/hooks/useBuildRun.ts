import { useCallback, useReducer, type RefObject } from "react";
import { Channel } from "@tauri-apps/api/core";
import { getCurrentWindow, UserAttentionType } from "@tauri-apps/api/window";
import { cancelBuild, cleanProject, runBuild, type Artifact, type Detection, type Options, type RunEvent } from "../lib/api";
import { formatMs } from "../lib/format";
import type { TerminalHandle } from "../components/Terminal";

export type StepStatus = "pending" | "running" | "ok" | "fail" | "skipped";

interface StepState {
  status: StepStatus;
  ms?: number;
}

export interface RunState {
  /** "cleaning" = build folders are being deleted; Rust holds the same busy slot as a build. */
  phase: "idle" | "running" | "cleaning" | "done";
  startedAt: number;
  /** The plan Rust actually ran (re-detected at click time). */
  plan: Detection | null;
  steps: Record<string, StepState>;
  artifacts: Artifact[];
  result: { ok: boolean; cancelled: boolean; ms: number; error: string | null } | null;
  /**
   * Bumped whenever a build or a clean touched the project folder. App uses it as an effect dep to
   * re-detect: install reasons and the cleanable-folder list both depend on what's on disk.
   */
  completedRuns: number;
}

type Action =
  | { type: "start"; at: number }
  | { type: "reset" }
  | { type: "cleanStart" }
  | { type: "cleanEnd" }
  | Exclude<RunEvent, { type: "output" }>;

const initial: RunState = {
  phase: "idle",
  startedAt: 0,
  plan: null,
  steps: {},
  artifacts: [],
  result: null,
  completedRuns: 0,
};

function reducer(s: RunState, a: Action): RunState {
  switch (a.type) {
    case "start":
      return { ...initial, completedRuns: s.completedRuns, phase: "running", startedAt: a.at };
    case "reset":
      return { ...initial, completedRuns: s.completedRuns };
    case "plan":
      return { ...s, plan: a.detection };
    case "stepStarted":
      return { ...s, steps: { ...s.steps, [a.id]: { status: "running" } } };
    case "stepFinished":
      return { ...s, steps: { ...s.steps, [a.id]: { status: a.ok ? "ok" : "fail", ms: a.ms } } };
    case "artifacts":
      return { ...s, artifacts: a.items };
    case "done":
      return {
        ...s,
        phase: "done",
        completedRuns: s.completedRuns + 1,
        result: { ok: a.ok, cancelled: a.cancelled, ms: a.ms, error: a.error },
      };
    case "cleanStart":
      // Drop the last run's plan/artifacts: they point into folders that are about to vanish.
      return { ...initial, completedRuns: s.completedRuns, phase: "cleaning" };
    case "cleanEnd":
      return { ...initial, completedRuns: s.completedRuns + 1 };
  }
}

/** Derived per-step status: anything never started in a finished run was skipped (no extra state to sync). */
export function statusOf(run: RunState, id: string): StepState {
  const st = run.steps[id];
  if (st) return st;
  return { status: run.phase === "done" ? "skipped" : "pending" };
}

const C = { dim: "\x1b[2m", amber: "\x1b[38;5;214m", green: "\x1b[32m", red: "\x1b[31m", bold: "\x1b[1m", reset: "\x1b[0m" };

export function useBuildRun(term: RefObject<TerminalHandle | null>) {
  const [run, dispatch] = useReducer(reducer, initial);

  const start = useCallback(
    async (path: string, options: Options) => {
      const t = term.current;
      t?.clear();
      dispatch({ type: "start", at: Date.now() });

      const channel = new Channel<RunEvent>();
      // Output goes straight to xterm and never through React state: a cargo build emits thousands
      // of chunks, and a setState per chunk would re-render the whole tree for each one.
      channel.onmessage = (ev) => {
        const w = term.current?.write;
        switch (ev.type) {
          case "output":
            w?.(ev.data);
            return;
          case "stepStarted":
            w?.(`\n${C.amber}${C.bold}❯ ${ev.command}${C.reset}\n`);
            break;
          case "stepFinished":
            w?.(ev.ok ? `${C.green}✓ done in ${formatMs(ev.ms)}${C.reset}\n` : `${C.red}✗ exit code ${ev.code ?? "?"}${C.reset}\n`);
            break;
          case "done":
            if (ev.ok) w?.(`\n${C.green}${C.bold}Build finished in ${formatMs(ev.ms)}${C.reset}\n`);
            else if (ev.cancelled) w?.(`\n${C.amber}Cancelled.${C.reset}\n`);
            else w?.(`\n${C.red}${C.bold}Build failed${ev.error ? `: ${ev.error}` : ""}${C.reset}\n`);
            // Long builds happen in the background; flash the taskbar when it's over.
            getCurrentWindow().requestUserAttention(UserAttentionType.Informational).catch(() => {});
            break;
        }
        dispatch(ev);
      };

      try {
        await runBuild(path, options, channel);
      } catch (e) {
        // Command rejected before streaming (already running, bad path, empty plan).
        const error = String(e);
        term.current?.write(`${C.red}${error}${C.reset}\n`);
        dispatch({ type: "done", ok: false, cancelled: false, ms: 0, error });
      }
    },
    [term],
  );

  const cancel = useCallback(() => {
    cancelBuild().catch(() => {});
  }, []);

  const clean = useCallback(
    async (path: string, options: Options) => {
      const t = term.current;
      t?.clear();
      dispatch({ type: "cleanStart" });
      t?.write(`${C.amber}${C.bold}❯ Removing build folders${C.reset}\n`);
      try {
        const report = await cleanProject(path, options);
        for (const p of report.removed) t?.write(`${C.green}✓ removed ${p}${C.reset}\n`);
        for (const f of report.failed) t?.write(`${C.red}✗ ${f.path}: ${f.error}${C.reset}\n`);
        if (report.removed.length + report.failed.length === 0) t?.write(`${C.dim}Nothing to remove.${C.reset}\n`);
        if (report.failed.length > 0) t?.write(`${C.dim}A locked file usually means the built app is still running.${C.reset}\n`);
      } catch (e) {
        // Rejected up front: bad path, or a build grabbed the busy slot first.
        t?.write(`${C.red}${String(e)}${C.reset}\n`);
      } finally {
        // Always bump, even on partial failure: whatever *was* deleted changes the plan.
        dispatch({ type: "cleanEnd" });
      }
    },
    [term],
  );

  const reset = useCallback(() => dispatch({ type: "reset" }), []);

  return { run, start, cancel, clean, reset };
}
