// synthdesk: casca tauri sobre a engine lutier.
// alem da identidade da engine, expoe save/load de projeto .synthproj
// (dialog nativo + io de arquivo); o grafo de modulos do frontend vai
// compilar pra .synth/.score nas proximas fases.
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

use tauri_plugin_dialog::DialogExt;

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

#[derive(serde::Serialize)]
struct LoadedProject {
    path: String,
    json: String,
}

/// salva o projeto: com `path` grava direto (autosave/re-save); sem
/// `path` abre o dialog nativo. retorna o caminho gravado (None =
/// usuario cancelou).
#[tauri::command]
fn save_project(
    app: tauri::AppHandle,
    json: String,
    path: Option<String>,
    name: String,
) -> Result<Option<String>, String> {
    let target = match path {
        Some(p) if !p.is_empty() => std::path::PathBuf::from(p),
        _ => {
            let suggested = if name.is_empty() { "projeto.synthproj".into() } else { name };
            match app
                .dialog()
                .file()
                .add_filter("synthdesk project", &["synthproj"])
                .set_file_name(&suggested)
                .blocking_save_file()
            {
                Some(f) => f.into_path().map_err(|e| e.to_string())?,
                None => return Ok(None),
            }
        }
    };
    std::fs::write(&target, json).map_err(|e| e.to_string())?;
    Ok(Some(target.to_string_lossy().into_owned()))
}

/// abre o dialog nativo e devolve caminho + conteudo (None = cancelou)
#[tauri::command]
fn load_project(app: tauri::AppHandle) -> Result<Option<LoadedProject>, String> {
    let picked = app
        .dialog()
        .file()
        .add_filter("synthdesk project", &["synthproj"])
        .blocking_pick_file();
    let Some(f) = picked else { return Ok(None) };
    let path = f.into_path().map_err(|e| e.to_string())?;
    let json = std::fs::read_to_string(&path).map_err(|e| e.to_string())?;
    Ok(Some(LoadedProject { path: path.to_string_lossy().into_owned(), json }))
}

fn main() {
    tauri::Builder::default()
        .plugin(tauri_plugin_dialog::init())
        .invoke_handler(tauri::generate_handler![engine_info, save_project, load_project])
        .run(tauri::generate_context!())
        .expect("erro ao iniciar synthdesk");
}
