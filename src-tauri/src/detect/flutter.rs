//! Flutter apps and standalone Dart packages.
//! Supports Windows desktop binaries (`flutter build windows --release`), Android APKs (`flutter build apk`),
//! web, and compiled Dart command-line executables (`dart compile exe`).

use super::{Ctx, Detected, Note, Stack};
use crate::plan::{Step, StepKind, sv};

pub fn detect(ctx: &Ctx) -> Option<Detected> {
    let pubspec_text = ctx.read("pubspec.yaml")?;
    let is_flutter = pubspec_text.contains("sdk: flutter")
        || pubspec_text.contains("flutter:")
        || ctx.has("lib/main.dart");

    let pkg_name = pubspec_text
        .lines()
        .map(str::trim)
        .find(|l| l.starts_with("name:"))
        .and_then(|l| l.split_once(':'))
        .map(|(_, v)| v.trim().trim_matches(['"', '\'']).to_string())
        .unwrap_or_else(|| {
            ctx.root
                .file_name()
                .map(|n| n.to_string_lossy().into_owned())
                .unwrap_or_else(|| "app".into())
        });

    if is_flutter {
        let mut d = Detected::new(Stack::Flutter, 75);
        d.tags = vec!["Flutter".into(), "Dart".into()];
        d.tools = vec!["flutter".into()];

        let marker = ctx.mtime(".dart_tool/package_config.json");
        let stale = ctx.opts.force_install
            || marker.is_none()
            || matches!((ctx.mtime("pubspec.yaml"), marker), (Some(p), Some(m)) if p > m)
            || matches!((ctx.mtime("pubspec.lock"), marker), (Some(l), Some(m)) if l > m);

        if !ctx.opts.skip_install && stale {
            let reason = if marker.is_none() {
                ".dart_tool is missing"
            } else {
                "pubspec changed since last install"
            };
            d.steps.push(
                Step::exec(StepKind::Install, "flutter pub get", "flutter", sv(&["pub", "get"]))
                    .reason(format!("{reason}, so Faber runs `flutter pub get`")),
            );
        }

        // Platform detection: prioritize Windows desktop on Windows OS
        if ctx.has("windows") {
            d.tags.push("Windows".into());
            d.steps.push(Step::exec(
                StepKind::Build,
                "flutter build windows",
                "flutter",
                sv(&["build", "windows", "--release"]),
            ));
            d.artifacts.push("build/windows/x64/runner/Release/*.exe".into());
            d.artifacts.push("build/windows/runner/Release/*.exe".into());
        } else if ctx.has("android") {
            d.tags.push("Android".into());
            d.steps.push(Step::exec(
                StepKind::Build,
                "flutter build apk",
                "flutter",
                sv(&["build", "apk", "--release"]),
            ));
            d.artifacts.push("build/app/outputs/flutter-apk/*.apk".into());
            d.artifacts.push("build/app/outputs/apk/release/*.apk".into());
        } else if ctx.has("web") {
            d.tags.push("Web".into());
            d.steps.push(Step::exec(
                StepKind::Build,
                "flutter build web",
                "flutter",
                sv(&["build", "web", "--release"]),
            ));
            d.artifacts.push("build/web/*".into());
        } else {
            d.steps.push(Step::exec(
                StepKind::Build,
                "flutter build",
                "flutter",
                sv(&["build"]),
            ));
        }

        d.clean.push("build".into());
        d.clean.push(".dart_tool".into());
        Some(d)
    } else {
        // Pure Dart project
        let mut d = Detected::new(Stack::Dart, 55);
        d.tags = vec!["Dart".into()];
        d.tools = vec!["dart".into()];

        let marker = ctx.mtime(".dart_tool/package_config.json");
        let stale = ctx.opts.force_install
            || marker.is_none()
            || matches!((ctx.mtime("pubspec.yaml"), marker), (Some(p), Some(m)) if p > m);

        if !ctx.opts.skip_install && stale {
            d.steps.push(
                Step::exec(StepKind::Install, "dart pub get", "dart", sv(&["pub", "get"]))
                    .reason("dependencies not resolved, so Faber runs `dart pub get`"),
            );
        }

        let entry = ["bin/main.dart", &format!("bin/{pkg_name}.dart")]
            .into_iter()
            .find(|e| ctx.has(e))
            .map(|s| s.to_string())
            .or_else(|| {
                ctx.glob("bin/*.dart")
                    .first()
                    .map(|p| crate::clean::rel(&ctx.root, p))
            });

        if let Some(entry_path) = entry {
            let out_exe = format!("bin/{pkg_name}.exe");
            d.steps.push(Step::exec(
                StepKind::Build,
                format!("dart compile exe {entry_path}"),
                "dart",
                sv(&["compile", "exe", &entry_path, "-o", &out_exe]),
            ));
            d.artifacts.push(out_exe);
        } else {
            d.notes.push(Note::info("Dart project without bin/ executable entry; running `dart test` or checking syntax."));
        }

        d.artifacts.push("bin/*.exe".into());
        d.clean.push(".dart_tool".into());
        d.clean.push("bin/*.exe".into());
        Some(d)
    }
}
