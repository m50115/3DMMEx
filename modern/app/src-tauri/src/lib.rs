mod commands;
mod state;
mod stream;

use state::AppState;

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_fs::init())
        .register_uri_scheme_protocol("stream", stream::handle)
        .manage(AppState::new())
        .invoke_handler(tauri::generate_handler![
            commands::open_file,
            commands::get_scene_list,
            commands::render_demo_frame,
            commands::render_scene_frame,
            commands::list_sounds,
            commands::play_sound,
        ])
        .run(tauri::generate_context!())
        .expect("error running 3DMMEx application");
}
