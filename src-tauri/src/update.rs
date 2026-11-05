//! The launcher's own updates: the latest release of prstance/wp-launcher on
//! GitHub (its latest.json, signed with the key whose public half is in
//! tauri.conf.json). Only when the player asks, from Settings › About.

use crate::game::is_playing;
use crate::i18n::tr;
use serde::Serialize;
use tauri::AppHandle;
use tauri_plugin_updater::UpdaterExt;

#[derive(Serialize)]
pub struct Update {
    version: String,
    notes: Option<String>,
}

async fn latest(app: &AppHandle) -> Result<Option<tauri_plugin_updater::Update>, String> {
    let updater = app.updater().map_err(|e| e.to_string())?;
    updater.check().await.map_err(|e| tr!("Impossible de vérifier les mises à jour : {e}", e = e))
}

/// The newer version on GitHub, if there is one.
#[tauri::command]
pub async fn launcher_update_check(app: AppHandle) -> Result<Option<Update>, String> {
    Ok(latest(&app).await?.map(|u| Update { version: u.version, notes: u.body }))
}

/// Downloads it, installs it and starts it.
#[tauri::command]
pub async fn launcher_update_install(app: AppHandle) -> Result<(), String> {
    if is_playing(&app) {
        return Err(tr!("Arrête la partie avant de mettre le launcher à jour."));
    }
    let update = latest(&app).await?.ok_or_else(|| tr!("Le launcher est déjà à jour."))?;
    update.download_and_install(|_, _| {}, || {}).await.map_err(|e| tr!("La mise à jour a échoué : {e}", e = e))?;
    app.restart()
}
