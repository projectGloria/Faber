//! CMake and Makefile native C/C++ projects.
//! Configures and builds Release binaries via `cmake -B build` and `cmake --build build --config Release`.

use super::{Ctx, Detected, Stack};
use crate::plan::{Step, StepKind, sv};

pub fn detect(ctx: &Ctx) -> Option<Detected> {
    if ctx.has("CMakeLists.txt") {
        let mut d = Detected::new(Stack::Cmake, 48);
        d.tags = vec!["CMake".into(), "C/C++".into()];
        d.tools = vec!["cmake".into()];

        if !ctx.has("build/CMakeCache.txt") || ctx.opts.force_install {
            d.steps.push(Step::exec(
                StepKind::Install,
                "cmake configure (Release)",
                "cmake",
                sv(&["-B", "build", "-DCMAKE_BUILD_TYPE=Release"]),
            ));
        }

        d.steps.push(Step::exec(
            StepKind::Build,
            "cmake build (Release)",
            "cmake",
            sv(&["--build", "build", "--config", "Release"]),
        ));

        d.artifacts = sv(&[
            "build/Release/*.exe",
            "build/*.exe",
            "build/bin/*.exe",
            "bin/*.exe",
            "*.exe",
        ]);
        d.clean = sv(&["build", "bin"]);
        return Some(d);
    }

    if ctx.has("Makefile") {
        let mut d = Detected::new(Stack::Cmake, 47);
        d.tags = vec!["Make".into(), "C/C++".into()];
        d.tools = vec!["make".into()];
        d.steps.push(Step::exec(StepKind::Build, "make", "make", vec![]));
        d.artifacts = sv(&["*.exe", "bin/*.exe", "build/*.exe"]);
        d.clean = sv(&["build", "bin"]);
        return Some(d);
    }

    None
}
