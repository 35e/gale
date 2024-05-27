use std::path::PathBuf;

use crate::command_util::Result;

use anyhow::anyhow;
use tauri::AppHandle;
use uuid::Uuid;
use super::{r2modman, ImportData};

#[tauri::command]
pub async fn import_data(data: ImportData, app: AppHandle) -> Result<()> {
    super::import_data(data, &app).await?;

    Ok(())
}

#[tauri::command]
pub async fn import_code(key: &str, app: AppHandle) -> Result<ImportData> {
    let key = Uuid::parse_str(key).map_err(|_| anyhow!("invalid code format"))?;
    let data = super::import_code(key, &app).await?;

    Ok(data)
}

#[tauri::command]
pub fn import_file(path: PathBuf, app: AppHandle) -> Result<ImportData> {
    let data = super::import_file_from_path(path, &app)?;

    Ok(data)
}

#[tauri::command]
pub async fn import_local_mod(path: PathBuf, app: AppHandle) -> Result<()> {
    super::import_local_mod(path, &app).await?;

    Ok(())
}

#[tauri::command]
pub async fn import_r2modman(app: AppHandle) -> Result<()> {
    let path = PathBuf::from(r"C:\Users\bobbo\AppData\Roaming\r2modmanPlus-local");
    r2modman::import_r2modman(&path, &app).await?;

    Ok(())
}
