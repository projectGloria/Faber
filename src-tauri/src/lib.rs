mod clean;
mod commands;
mod detect;
mod plan;
mod runner;
mod toolchain;

use tauri::Manager;

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_opener::init())
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_store::Builder::new().build())
        .manage(runner::RunState::default())
        .invoke_handler(tauri::generate_handler![
            commands::detect_project,
            commands::run_build,
            commands::cancel_build,
            commands::clean_project
        ])
        .on_window_event(|window, event| {
            // Closing Faber mid-build must not leave an orphaned cargo/node tree holding locks.
            if let tauri::WindowEvent::CloseRequested { .. } = event {
                window.state::<runner::RunState>().cancel();
            }
        })
        .run(tauri::generate_context!())
        .expect("error while running Faber");
}
