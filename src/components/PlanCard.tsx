import { useEffect, useState } from "react";
import { STACK_LABEL, type Detection, type Note, type Options } from "../lib/api";
import { formatMs } from "../lib/format";
import { statusOf, type RunState, type StepStatus } from "../hooks/useBuildRun";

interface Props {
  detection: Detection;
  /** Existing build folders from the live detection (see App for why not `detection.cleanable`). */
  cleanable: string[];
  options: Options;
  onOptions: (o: Options) => void;
  run: RunState;
  onBuild: () => void;
  onCancel: () => void;
  onClean: (folders: string[]) => void;
}

const ICON: Record<StepStatus, string> = { pending: "○", running: "", ok: "✓", fail: "✗", skipped: "–" };

export function PlanCard({ detection, cleanable, options, onOptions, run, onBuild, onCancel, onClean }: Props) {
  const running = run.phase === "running";
  const cleaning = run.phase === "cleaning";
  // `busy` locks inputs; `running` alone decides Cancel vs Build (a delete can't be cancelled midway).
  const busy = running || cleaning;
  const set = (patch: Partial<Options>) => onOptions({ ...options, ...patch });

  // Install reasons are the "why" messages ("node_modules missing → npm i"): surface them as notices.
  const reasons: Note[] = detection.plan.steps.filter((s) => s.reason).map((s) => ({ level: "info", text: s.reason! }));
  const notes = [...reasons, ...detection.notes];
  const blocked = detection.plan.steps.length === 0 || detection.notes.some((n) => n.level === "error");

  return (
    <section className="card plan">
      <div className="plan-head">
        <div className="stack-row">
          {detection.stack && <span className={`stack-badge stack-${detection.stack}`}>{STACK_LABEL[detection.stack]}</span>}
          {detection.packageManager && <span className="tag">{detection.packageManager}</span>}
          {detection.tags.map((t) => (
            <span className="tag" key={t}>{t}</span>
          ))}
        </div>
        {detection.alternatives.length > 0 && (
          <div className="muted small">Also looks like: {detection.alternatives.map((a) => STACK_LABEL[a]).join(", ")}</div>
        )}
      </div>

      <div className="targets">
        <Toggle label="Portable" on={options.portable} disabled={busy} onChange={(v) => set({ portable: v })} />
        <Toggle label="Installer" on={options.installer} disabled={busy} onChange={(v) => set({ installer: v })} />
      </div>

      {notes.length > 0 && (
        <ul className="notes">
          {notes.map((n, i) => (
            <li key={i} className={`note note-${n.level}`}>
              <span className="note-dot" />
              {n.text}
            </li>
          ))}
        </ul>
      )}

      <ol className="steps">
        {detection.plan.steps.map((step) => {
          const st = statusOf(run, step.id);
          return (
            <li key={step.id} className={`step step-${st.status}`}>
              <span className="step-icon">{st.status === "running" ? <span className="spinner" /> : ICON[st.status]}</span>
              <div className="step-body">
                <div className="step-label">
                  {step.label}
                  {st.ms !== undefined && <span className="step-ms">{formatMs(st.ms)}</span>}
                </div>
                <code className="step-cmd" title={step.command}>{step.command}</code>
              </div>
            </li>
          );
        })}
      </ol>

      <details className="advanced">
        <summary>Advanced</summary>
        <label className="check">
          <input type="checkbox" checked={options.skipInstall} disabled={busy} onChange={(e) => set({ skipInstall: e.target.checked, forceInstall: false })} />
          Skip dependency install
        </label>
        <label className="check">
          <input type="checkbox" checked={options.forceInstall} disabled={busy} onChange={(e) => set({ forceInstall: e.target.checked, skipInstall: false })} />
          Force reinstall
        </label>
        <ExtraArgs key={detection.root} value={options.extraArgs} disabled={busy} onCommit={(v) => set({ extraArgs: v })} />
      </details>

      <div className="actions">
        {running ? (
          <button className="btn btn-danger" onClick={onCancel}>
            Cancel <Elapsed since={run.startedAt} />
          </button>
        ) : (
          <button className="btn btn-primary" onClick={onBuild} disabled={blocked || cleaning}>
            Build
          </button>
        )}
        <button
          className="btn btn-ghost btn-clean"
          onClick={() => onClean(cleanable)}
          disabled={busy || cleanable.length === 0}
          title={cleanable.length ? `Deletes: ${cleanable.join(", ")}` : "No build folders on disk"}
        >
          {cleaning ? <><span className="spinner" /> Removing…</> : `Remove uninstallers${cleanable.length ? ` (${cleanable.length})` : ""}`}
        </button>
      </div>
    </section>
  );
}

function Toggle({ label, on, disabled, onChange }: { label: string; on: boolean; disabled?: boolean; onChange: (v: boolean) => void }) {
  return (
    <button className={`toggle ${on ? "on" : ""}`} disabled={disabled} onClick={() => onChange(!on)} aria-pressed={on}>
      <span className="toggle-knob" />
      {label}
    </button>
  );
}

/**
 * Draft state commits on blur/Enter instead of every keystroke: each commit re-runs
 * detection in Rust, and half-typed args like `--feat` would flash nonsense plans.
 * The parent keys this by project root, so switching projects remounts it with a fresh
 * draft. No "sync prop into state" effect, which would render once with the stale draft first.
 */
function ExtraArgs({ value, disabled, onCommit }: { value: string; disabled?: boolean; onCommit: (v: string) => void }) {
  const [draft, setDraft] = useState(value);
  const commit = () => draft !== value && onCommit(draft.trim());
  return (
    <label className="field">
      <span>Extra args for the build step</span>
      <input
        value={draft}
        disabled={disabled}
        placeholder="e.g. --features fast"
        spellCheck={false}
        onChange={(e) => setDraft(e.target.value)}
        onBlur={commit}
        onKeyDown={(e) => e.key === "Enter" && commit()}
      />
    </label>
  );
}

/** Owns its own ticking state so only this span re-renders every second, not the card. */
function Elapsed({ since }: { since: number }) {
  const [now, setNow] = useState(() => Date.now());
  useEffect(() => {
    const id = setInterval(() => setNow(Date.now()), 1000);
    return () => clearInterval(id);
  }, []);
  return <span className="elapsed">{formatMs(Math.max(0, Math.floor((now - since) / 1000) * 1000))}</span>;
}
