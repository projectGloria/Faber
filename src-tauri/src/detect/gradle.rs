//! Gradle projects and Android apps (Orca, Wallupus, Kotlin/Java Android & desktop apps).
//! Uses the repo's bundled `gradlew.bat` wrapper when available.

use super::{Ctx, Detected, Stack};
use crate::plan::{Step, StepKind};

pub fn detect(ctx: &Ctx) -> Option<Detected> {
    let has_gradle_build = ctx.has("build.gradle.kts")
        || ctx.has("build.gradle")
        || ctx.has("settings.gradle.kts")
        || ctx.has("settings.gradle")
        || ctx.has("gradlew.bat")
        || ctx.has("gradlew");

    if !has_gradle_build {
        return None;
    }

    let is_android = ctx.has("app/src/main/AndroidManifest.xml")
        || ctx.has("src/main/AndroidManifest.xml")
        || ctx.read("build.gradle.kts").is_some_and(|s| s.contains("com.android"))
        || ctx.read("build.gradle").is_some_and(|s| s.contains("com.android"))
        || ctx.read("settings.gradle.kts").is_some_and(|s| s.contains("android"))
        || ctx.read("app/build.gradle.kts").is_some_and(|s| s.contains("com.android"))
        || ctx.read("app/build.gradle").is_some_and(|s| s.contains("com.android"));

    let is_kotlin = ctx.has("build.gradle.kts")
        || ctx.has("settings.gradle.kts")
        || ctx.has("app/build.gradle.kts")
        || !ctx.glob("src/**/*.kt").is_empty()
        || !ctx.glob("app/src/**/*.kt").is_empty();

    let mut d = Detected::new(Stack::Gradle, 65);
    if is_android {
        d.tags.push("Android".into());
    }
    if is_kotlin {
        d.tags.push("Kotlin".into());
    } else {
        d.tags.push("Java".into());
    }
    d.tags.push("Gradle".into());

    let has_wrapper = ctx.has("gradlew.bat");
    let task = if is_android {
        "assembleRelease"
    } else {
        "build"
    };

    let (prog, args) = if has_wrapper {
        ("cmd".to_string(), vec!["/C".into(), ".\\gradlew.bat".into(), task.into()])
    } else {
        d.tools.push("gradle".into());
        ("gradle".to_string(), vec![task.into()])
    };

    d.steps.push(Step::exec(
        StepKind::Build,
        if is_android { format!("Gradle {task} (APK)") } else { "Gradle build".into() },
        prog,
        args,
    ));

    d.artifacts = vec![
        "app/build/outputs/apk/release/*.apk".into(),
        "app/build/outputs/apk/debug/*.apk".into(),
        "build/outputs/apk/release/*.apk".into(),
        "build/outputs/apk/debug/*.apk".into(),
        "*/build/outputs/apk/*/*.apk".into(),
        "app/build/outputs/bundle/release/*.aab".into(),
        "build/libs/*.jar".into(),
        "app/build/libs/*.jar".into(),
    ];

    d.clean = vec![
        "build".into(),
        "app/build".into(),
        ".gradle".into(),
        "*/build".into(),
    ];

    Some(d)
}
