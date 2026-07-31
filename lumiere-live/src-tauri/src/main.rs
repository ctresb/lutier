// lumiere-live: casca tauri do visualizador. o frontend pede a
// lista de inputs, escolhe um e recebe frames de analise ~60hz.
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod audio;

fn main() {
    tauri::Builder::default()
        .manage(audio::Capture::default())
        .invoke_handler(tauri::generate_handler![
            audio::list_inputs,
            audio::start_capture,
            audio::start_default,
        ])
        .run(tauri::generate_context!())
        .expect("erro ao iniciar o lumiere-live");
}
