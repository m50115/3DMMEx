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
            commands::get_scene_actors,
            commands::update_actor_position,
            commands::update_actor_frame_range,
            commands::update_actor_orientation,
            commands::save_file,
            commands::list_templates,
            commands::add_actor,
            commands::remove_actor,
            commands::create_movie,
        ])
        .run(tauri::generate_context!())
        .expect("error running 3DMMEx application");
}
