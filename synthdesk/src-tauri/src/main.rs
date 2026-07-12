// synthdesk: casca tauri sobre a engine lutier.
// por enquanto expoe so a identidade da engine; o grafo de modulos
// do frontend vai compilar pra .synth/.score nas proximas fases.
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

#[derive(serde::Serialize)]
struct EngineInfo {
    name: &'static str,
    version: &'static str,
}

#[tauri::command]
fn engine_info() -> EngineInfo {
    // garante o link real com o crate lutier em tempo de compilacao
    let _ = std::any::type_name::<lutier::engine::StateStore>;
    EngineInfo { name: "lutier", version: env!("CARGO_PKG_VERSION") }
}

fn main() {
    tauri::Builder::default()
        .invoke_handler(tauri::generate_handler![engine_info])
        .run(tauri::generate_context!())
        .expect("erro ao iniciar synthdesk");
}
