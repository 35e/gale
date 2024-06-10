use std::{collections::HashMap, env, fs, path::PathBuf, sync::Mutex};

use anyhow::{anyhow, bail, ensure, Context, Result};
use serde::{Deserialize, Serialize};
use tauri::{AppHandle, Manager};

use crate::{
    manager::{installer, launcher::LaunchMode},
    util::{self, error::IoResultExt},
};

pub mod commands;

pub fn setup(app: &AppHandle) -> Result<()> {
    let prefs = Prefs::create(app)?;

    app.manage(Mutex::new(prefs));

    Ok(())
}

#[derive(Serialize, Deserialize, Clone, Debug)]
#[serde(untagged, rename_all = "camelCase")]
pub enum PrefValue {
    Float(f32),
    Path(PathBuf),
    LaunchMode(LaunchMode),
    Bool(bool),
}

impl PrefValue {
    pub fn as_path(&self) -> Option<&PathBuf> {
        match self {
            PrefValue::Path(path) => Some(path),
            _ => None,
        }
    }

    pub fn as_bool(&self) -> Option<bool> {
        match self {
            PrefValue::Bool(b) => Some(*b),
            _ => None,
        }
    }
}

pub struct Prefs {
    path: PathBuf,
    is_first_run: bool,
    map: HashMap<String, PrefValue>,
}

impl Prefs {
    pub fn create(app: &AppHandle) -> Result<Self> {
        let path_resolver = app.path_resolver();
        let config_path = path_resolver
            .app_config_dir()
            .context("failed to resolve preference directory")?;
        fs::create_dir_all(&config_path)?;

        let path = config_path.join("prefs.json");
        let is_first_run = !path.exists();
        let mut map = path
            .exists()
            .then(|| util::fs::read_json::<HashMap<String, PrefValue>>(&path))
            .transpose()?
            .unwrap_or_default();

        if map.get("steam_exe_path").is_none() {
            let path = PathBuf::from(match env::consts::OS {
                "windows" => r"C:\Program Files (x86)\Steam\steam.exe",
                "macos" => "/Applications/Steam.app/Contents/MacOS/Steam",
                "linux" => "/usr/bin/steam",
                _ => "",
            });

            if path.exists() {
                map.insert("steam_exe_path".to_owned(), PrefValue::Path(path));
            }
        }

        if map.get("steam_game_dir").is_none() {
            let path = match env::consts::OS {
                "windows" => match map.get("steam_exe_path") {
                    Some(PrefValue::Path(exe_path)) => {
                        exe_path.parent().map(|parent| parent.to_path_buf())
                    }
                    _ => Some(r"C:\Program Files (x86)\Steam".into()),
                },
                "macos" => Some("~/Library/Application Support/Steam/steamapps/common".into()),
                "linux" => tauri::api::path::data_dir().map(|data_dir| data_dir.join("Steam")),
                _ => None,
            };

            if let Some(path) = path {
                if path.exists() {
                    map.insert("steam_game_dir".to_owned(), PrefValue::Path(path));
                }
            }
        }

        insert_default_path(&mut map, "data_dir", || {
            path_resolver
                .app_data_dir()
                .context("failed to resolve app data dir")
        })?;

        insert_default_path(&mut map, "cache_dir", || {
            path_resolver
                .app_cache_dir()
                .context("failed to resolve app cache dir")
                .map(|cache_dir| cache_dir.join("cache"))
        })?;

        insert_default_path(&mut map, "temp_dir", || {
            path_resolver
                .app_cache_dir()
                .context("failed to resolve app temp dir")
                .map(|cache_dir| cache_dir.join("temp"))
        })?;

        map.entry("launch_mode".to_owned())
            .or_insert(PrefValue::LaunchMode(LaunchMode::Steam));

        map.entry("enable_mod_cache".to_owned())
            .or_insert(PrefValue::Bool(true));

        match map.get("zoom_factor") {
            Some(value) => {
                let zoom_factor = match value {
                    PrefValue::Float(f) => *f,
                    _ => 1.0,
                };

                let window = app.get_window("main").unwrap();
                util::window::zoom(&window, zoom_factor as f64).ok();
            }
            None => {
                map.insert("zoom_factor".to_owned(), PrefValue::Float(1.0));
            }
        }

        let prefs = Self {
            path,
            is_first_run,
            map,
        };

        prefs.save()?;

        return Ok(prefs);

        fn insert_default_path(
            map: &mut HashMap<String, PrefValue>,
            key: &str,
            default: impl FnOnce() -> Result<PathBuf>,
        ) -> Result<()> {
            if map.get(key).is_none() {
                map.insert(key.to_owned(), PrefValue::Path(default()?));
            }

            Ok(())
        }
    }

    pub fn save(&self) -> Result<()> {
        let json = serde_json::to_string_pretty(&self.map)?;
        fs::write(&self.path, json).fs_context("saving preferences", &self.path)?;
        Ok(())
    }

    pub fn get_path_or_err(&self, key: &str) -> Result<&PathBuf> {
        self.get_or_err(key)?
            .as_path()
            .ok_or_else(|| anyhow!("pref {} is not a path", key))
    }

    pub fn get_or_err(&self, key: &str) -> Result<&PrefValue> {
        self.get(key)
            .ok_or_else(|| anyhow!("pref {} not found", key))
    }

    pub fn get<'a>(&'a self, key: &str) -> Option<&'a PrefValue> {
        self.map.get(key)
    }

    fn set(&mut self, key: &str, value: PrefValue, window: Option<&tauri::Window>) -> Result<()> {
        match key {
            "cache_dir" | "temp_dir" => self.move_dir(key, &value, None)?,
            "data_dir" => self.move_dir(key, &value, Some(&["prefs.json"]))?,
            "zoom_factor" => match value {
                PrefValue::Float(factor) => {
                    if let Some(window) = window {
                        util::window::zoom(window, factor as f64).map_err(|e| anyhow!(e))?;
                    }
                }
                _ => bail!("value is not a float"),
            },
            "enable_mod_cache" => match value {
                PrefValue::Bool(false) => installer::clear_cache(self)?,
                PrefValue::Bool(true) => (),
                _ => bail!("value is not a bool"),
            },
            _ => (),
        };

        self.set_raw(key, value)
    }

    fn set_raw(&mut self, key: impl Into<String>, value: PrefValue) -> Result<()> {
        self.map.insert(key.into(), value);
        self.save()?;
        Ok(())
    }

    fn move_dir(&mut self, key: &str, value: &PrefValue, excludes: Option<&[&str]>) -> Result<()> {
        let new_path = match value {
            PrefValue::Path(path) => path,
            _ => bail!("value is not a path"),
        };

        let old_path = match self.get(key) {
            Some(PrefValue::Path(path)) => Some(path),
            _ => None,
        };

        ensure!(
            old_path != Some(new_path),
            "{} is already set to {}",
            key,
            new_path.display()
        );
        ensure!(new_path.exists(), "{} does not exist", new_path.display());
        ensure!(
            new_path.is_dir(),
            "{} is not a directory",
            new_path.display()
        );

        match old_path {
            Some(old_path) if old_path.exists() => {
                util::fs::copy_dir(old_path, new_path, false)?;
                if let Some(excludes) = excludes {
                    for exclude in excludes {
                        fs::remove_file(new_path.join(exclude)).ok();
                    }

                    for entry in old_path.read_dir()? {
                        let entry = entry?;

                        if !excludes.iter().any(|exclude| entry.file_name() == *exclude) {
                            if entry.file_type()?.is_dir() {
                                fs::remove_dir_all(entry.path())?;
                            } else {
                                fs::remove_file(entry.path())?;
                            }
                        }
                    }
                } else {
                    fs::remove_dir_all(old_path)?;
                }
            }
            _ => {
                fs::create_dir_all(new_path)?;
            }
        }

        Ok(())
    }
}
