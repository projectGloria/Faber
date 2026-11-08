//! .NET SDK projects (`dotnet publish` single-file) and Visual Studio C++ / legacy
//! .NET Framework projects (MSBuild located via vswhere).

use super::{Ctx, Detected, Note, Stack};
use crate::plan::{Step, StepKind, sv};
use crate::toolchain;
use std::path::PathBuf;

pub fn detect(ctx: &Ctx) -> Option<Detected> {
    let slns: Vec<PathBuf> = [ctx.glob("*.sln"), ctx.glob("*.slnx")].concat();
    let csprojs: Vec<PathBuf> = [ctx.glob("*.csproj"), ctx.glob("*/*.csproj"), ctx.glob("*/*/*.csproj")].concat();
    let vcx: Vec<PathBuf> = [ctx.glob("*.vcxproj"), ctx.glob("*/*.vcxproj"), ctx.glob("*/*/*.vcxproj")].concat();
    if slns.is_empty() && csprojs.is_empty() && vcx.is_empty() {
        return None;
    }

    // SDK-style projects start with `<Project Sdk=`; old-style ones need full MSBuild.
    let sdk_style = |p: &PathBuf| std::fs::read_to_string(p).is_ok_and(|s| s.contains("<Project Sdk="));
    let needs_msbuild = !vcx.is_empty() || (!csprojs.is_empty() && !csprojs.iter().all(sdk_style));

    let rel = |p: &PathBuf| crate::clean::rel(&ctx.root, p);

    if needs_msbuild {
        let mut d = Detected::new(Stack::Msbuild, 60);
        d.tags = vec![if vcx.is_empty() { ".NET Framework".into() } else { "C++".into() }, "Visual Studio".into()];
        let target = slns.first().or(vcx.first()).or(csprojs.first()).map(rel).unwrap_or_default();
        let mut args = vec![target, "/m".into(), "/p:Configuration=Release".into()];
        if !vcx.is_empty() {
            args.push("/p:Platform=x64".into());
        }
        match toolchain::msbuild() {
            Some(msb) => d.steps.push(Step::exec(StepKind::Build, "MSBuild Release", msb.to_string_lossy(), args)),
            None => d.notes.push(Note::error("MSBuild not found (no VS / Build Tools detected via vswhere).")),
        }
        d.artifacts = sv(&["x64/Release/*.exe", "*/x64/Release/*.exe", "bin/Release/*.exe", "bin/Release/*/*.exe", "bin/x64/Release/*.exe"]);
        d.clean = sv(&["bin", "obj", "x64", "Release", "*/bin", "*/obj", "*/x64", "*/Release"]);
        return Some(d);
    }

    let mut d = Detected::new(Stack::Dotnet, 60);
    d.tags = vec![".NET".into()];
    d.tools = vec!["dotnet".into()];
    // One csproj, so publish it directly; otherwise let the solution decide.
    let target = if csprojs.len() == 1 { rel(&csprojs[0]) } else { slns.first().map(rel).unwrap_or_else(|| rel(&csprojs[0])) };

    let has_assets = ctx.has("obj/project.assets.json") || !ctx.glob("*/obj/project.assets.json").is_empty();
    if !ctx.opts.skip_install && (ctx.opts.force_install || !has_assets) {
        d.steps.push(
            Step::exec(StepKind::Install, "dotnet restore", "dotnet", vec!["restore".into(), target.clone()])
                .reason("NuGet packages aren't restored yet (project.assets.json missing)"),
        );
    }
    // Framework-dependent single file: small exe, uses the runtime already on this machine.
    d.steps.push(Step::exec(
        StepKind::Build,
        "dotnet publish (single-file)",
        "dotnet",
        vec![
            "publish".into(),
            target,
            "-c".into(),
            "Release".into(),
            "-r".into(),
            "win-x64".into(),
            "--self-contained".into(),
            "false".into(),
            "-p:PublishSingleFile=true".into(),
        ],
    ));
    // No `-o`: the SDK's native layout is bin/Release/<tfm>/<rid>/publish (per project for a .sln).
    d.artifacts = sv(&["bin/Release/*/win-x64/publish/*.exe", "*/bin/Release/*/win-x64/publish/*.exe"]);
    d.clean = sv(&["bin", "obj", "*/bin", "*/obj"]);
    if ctx.opts.installer && ctx.glob("*.iss").is_empty() {
        d.notes.push(Note::info("No installer project (.iss), so the single-file exe is the deliverable."));
    }
    Some(d)
}
