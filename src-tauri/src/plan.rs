//! The build plan: an ordered list of steps a detector produced for one project.
//! The UI renders it as a checklist before anything runs; the runner executes it.

use serde::{Deserialize, Serialize};
use std::path::Path;

#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub enum StepKind {
    Install,
    Build,
    Package,
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
#[serde(tag = "type", rename_all = "camelCase", rename_all_fields = "camelCase")]
pub enum Action {
    /// Spawn a process. `program` is a bare name (resolved through PATH/PATHEXT at run time,
    /// so `npm` finds `npm.cmd`) or an absolute path (venv python, MSBuild from vswhere).
    Exec {
        program: String,
        args: Vec<String>,
        env: Vec<(String, String)>,
    },
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct Step {
    pub id: String,
    pub kind: StepKind,
    pub label: String,
    /// Human-readable command line, precomputed so the UI never re-implements quoting rules.
    pub command: String,
    /// Why this step exists, e.g. "node_modules missing". Shown as a notice in the UI.
    pub reason: Option<String>,
    pub action: Action,
}

impl Step {
    pub fn exec(kind: StepKind, label: impl Into<String>, program: impl Into<String>, args: Vec<String>) -> Self {
        let program = program.into();
        Step {
            id: String::new(), // assigned in `BuildPlan::finalize` so ids are unique and ordered
            kind,
            label: label.into(),
            command: display_command(&program, &args),
            reason: None,
            action: Action::Exec { program, args, env: Vec::new() },
        }
    }

    pub fn reason(mut self, reason: impl Into<String>) -> Self {
        self.reason = Some(reason.into());
        self
    }

    /// Append user-supplied extra args (from overrides) to an Exec step.
    pub fn push_args(&mut self, extra: &[String]) {
        let Action::Exec { program, args, .. } = &mut self.action;
        args.extend_from_slice(extra);
        self.command = display_command(program, args);
    }
}

#[derive(Serialize, Deserialize, Clone, Debug, Default, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct BuildPlan {
    pub steps: Vec<Step>,
    /// Glob patterns (relative to the project root) for produced files. After a run only
    /// matches modified during that run are reported, so stale artifacts from older builds never show.
    pub artifacts: Vec<String>,
    /// Exact paths listed after a successful run regardless of mtime, for outputs the build
    /// verifies as current without necessarily rewriting them (cargo skips relinking an unchanged exe).
    pub pinned_artifacts: Vec<String>,
    /// Glob patterns for the build output folders "Remove uninstallers" deletes. Resolved and
    /// safety-checked in `clean.rs`; dependency folders (node_modules, .venv) never belong here.
    pub clean: Vec<String>,
}

impl BuildPlan {
    pub fn finalize(&mut self) {
        for (i, s) in self.steps.iter_mut().enumerate() {
            let slug: String = s
                .label
                .to_lowercase()
                .chars()
                .map(|c| if c.is_ascii_alphanumeric() { c } else { '-' })
                .collect();
            s.id = format!("{i}-{}", slug.trim_matches('-'));
        }
    }
}

pub fn sv(items: &[&str]) -> Vec<String> {
    items.iter().map(|s| s.to_string()).collect()
}

fn display_command(program: &str, args: &[String]) -> String {
    // Show just the file name for absolute paths (venv python, MSBuild) to keep the checklist readable.
    let prog = if Path::new(program).is_absolute() {
        Path::new(program)
            .file_name()
            .map(|f| f.to_string_lossy().into_owned())
            .unwrap_or_else(|| program.to_string())
    } else {
        program.to_string()
    };
    std::iter::once(prog)
        .chain(args.iter().map(|a| if a.contains(' ') { format!("\"{a}\"") } else { a.clone() }))
        .collect::<Vec<_>>()
        .join(" ")
}
