// Typed bridge to the Rust commands. Shapes mirror the serde types in src-tauri/src
// (camelCase fields, `type`-tagged enums), so keep both sides in sync when changing one.
import { Channel, invoke } from "@tauri-apps/api/core";

export type Stack = "tauri" | "electron" | "node" | "cargo" | "dotnet" | "msbuild" | "python" | "script";
export type StepKind = "install" | "build" | "package";
export type PackageManager = "npm" | "pnpm" | "yarn" | "bun";

export interface Step {
  id: string;
  kind: StepKind;
  label: string;
  command: string;
  reason: string | null;
}

export interface Note {
  level: "info" | "warn" | "error";
  text: string;
}

export interface Detection {
  root: string;
  name: string;
  stack: Stack | null;
  tags: string[];
  packageManager: PackageManager | null;
  plan: { steps: Step[]; artifacts: string[] };
  notes: Note[];
  alternatives: Stack[];
  /** Build folders that exist right now and "Remove uninstallers" would delete (root-relative). */
  cleanable: string[];
}

export interface CleanReport {
  removed: string[];
  failed: { path: string; error: string }[];
}

export interface Options {
  portable: boolean;
  installer: boolean;
  skipInstall: boolean;
  forceInstall: boolean;
  extraArgs: string;
}

export const DEFAULT_OPTIONS: Options = {
  portable: true,
  installer: true,
  skipInstall: false,
  forceInstall: false,
  extraArgs: "",
};

export interface Artifact {
  path: string;
  name: string;
  size: number;
  isDir: boolean;
}

export type RunEvent =
  | { type: "plan"; detection: Detection }
  | { type: "stepStarted"; id: string; command: string }
  | { type: "output"; id: string; data: string }
  | { type: "stepFinished"; id: string; code: number | null; ms: number; ok: boolean }
  | { type: "artifacts"; items: Artifact[] }
  | { type: "done"; ok: boolean; cancelled: boolean; ms: number; error: string | null };

export const detectProject = (path: string, options: Options) =>
  invoke<Detection>("detect_project", { path, options });

// Tauri maps the camelCase `onEvent` key to the Rust `on_event` parameter.
export const runBuild = (path: string, options: Options, onEvent: Channel<RunEvent>) =>
  invoke<void>("run_build", { path, options, onEvent });

export const cancelBuild = () => invoke<void>("cancel_build");

// Path + options only; Rust re-derives the folder list, so the webview can't request arbitrary deletes.
export const cleanProject = (path: string, options: Options) =>
  invoke<CleanReport>("clean_project", { path, options });

export const STACK_LABEL: Record<Stack, string> = {
  tauri: "Tauri",
  electron: "Electron",
  node: "Node",
  cargo: "Cargo",
  dotnet: ".NET",
  msbuild: "MSBuild",
  python: "Python",
  script: "Script",
};
