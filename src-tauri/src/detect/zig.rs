//! Zig projects (`build.zig`).
//! Compiles optimized executables with `zig build -Doptimize=ReleaseSafe`.

use super::{Ctx, Detected, Stack};
use crate::plan::{Step, StepKind, sv};

pub fn detect(ctx: &Ctx) -> Option<Detected> {
    if !ctx.has("build.zig") {
        return None;
    }

    let mut d = Detected::new(Stack::Zig, 53);
    d.tags = vec!["Zig".into()];
    d.tools = vec!["zig".into()];

    d.steps.push(Step::exec(
        StepKind::Build,
        "zig build (ReleaseSafe)",
        "zig",
        sv(&["build", "-Doptimize=ReleaseSafe"]),
    ));

    d.artifacts = sv(&["zig-out/bin/*.exe", "zig-out/bin/*"]);
    d.clean = sv(&["zig-out", ".zig-cache"]);
    Some(d)
}
