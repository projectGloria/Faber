//! Go modules and command-line applications.
//! Compiles release executables with stripped debug symbols via `go build -ldflags "-s -w"`.

use super::{Ctx, Detected, Stack};
use crate::plan::{Step, StepKind, sv};

pub fn detect(ctx: &Ctx) -> Option<Detected> {
    let has_go = ctx.has("go.mod") || ctx.has("main.go") || !ctx.glob("*.go").is_empty();
    if !has_go {
        return None;
    }

    let mut d = Detected::new(Stack::Go, 52);
    d.tags = vec!["Go".into()];
    d.tools = vec!["go".into()];

    let mod_name = ctx
        .read("go.mod")
        .and_then(|text| {
            text.lines()
                .find(|l| l.trim_start().starts_with("module "))
                .and_then(|l| l.split_whitespace().nth(1))
                .and_then(|m| m.split('/').next_back())
                .map(String::from)
        })
        .unwrap_or_else(|| {
            ctx.root
                .file_name()
                .map(|n| n.to_string_lossy().into_owned())
                .unwrap_or_else(|| "app".into())
        });

    if ctx.has("go.mod") && !ctx.has("vendor") && !ctx.opts.skip_install {
        let stale = ctx.opts.force_install || !ctx.has("go.sum");
        if stale {
            d.steps.push(
                Step::exec(StepKind::Install, "go mod download", "go", sv(&["mod", "download"]))
                    .reason("downloading Go module dependencies"),
            );
        }
    }

    let out_exe = format!("{mod_name}.exe");
    d.steps.push(Step::exec(
        StepKind::Build,
        format!("go build -o {out_exe}"),
        "go",
        sv(&["build", "-ldflags", "-s -w", "-o", &out_exe, "."]),
    ));

    d.artifacts = sv(&[&out_exe, "*.exe", "bin/*.exe"]);
    d.clean = sv(&[&out_exe, "bin"]);
    Some(d)
}
