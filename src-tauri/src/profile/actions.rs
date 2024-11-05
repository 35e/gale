use std::{fs, path::Path, sync::Mutex};

use anyhow::{ensure, Context, Result};
use itertools::Itertools;
use serde::{Deserialize, Serialize};
use tauri::{AppHandle, Manager};
use uuid::Uuid;
use walkdir::WalkDir;

use super::{Dependant, ManagedGame, Profile, ProfileModKind};
use crate::{
    game::SubdirMode,
    prefs::Prefs,
    profile::ModManager,
    thunderstore::Thunderstore,
    util::{
        self,
        error::IoResultExt,
        fs::{Overwrite, PathExt},
    },
};

#[derive(Serialize)]
#[serde(rename_all = "camelCase", tag = "type")]
pub enum ActionResult {
    Done,
    Confirm { dependants: Vec<Dependant> },
}

impl Profile {
    pub fn rename(&mut self, name: String) -> Result<()> {
        ensure!(
            Self::is_valid_name(&name),
            "invalid profile name '{}'",
            name
        );

        let new_path = self.path.parent().unwrap().join(&name);

        ensure!(
            !new_path.exists(),
            "profile with name '{}' already exists",
            name
        );

        fs::rename(&self.path, &new_path).fs_context("renaming profile directory", &self.path)?;

        self.name = name;
        self.path = new_path;

        Ok(())
    }

    pub fn remove_mod(&mut self, uuid: Uuid, thunderstore: &Thunderstore) -> Result<ActionResult> {
        if self.get_mod(uuid)?.enabled {
            if let Some(dependants) = self.check_dependants(uuid, thunderstore) {
                return Ok(ActionResult::Confirm { dependants });
            }
        }

        self.force_remove_mod(uuid)?;
        Ok(ActionResult::Done)
    }

    pub fn force_remove_mod(&mut self, uuid: Uuid) -> Result<()> {
        let index = self.index_of(uuid)?;

        self.scan_mod(&self.mods[index].kind, |dir| {
            fs::remove_dir_all(dir).fs_context("removing mod directory", dir)
        })?;

        self.mods.remove(index);

        Ok(())
    }

    pub fn toggle_mod(&mut self, uuid: Uuid, thunderstore: &Thunderstore) -> Result<ActionResult> {
        let dependants = match self.get_mod(uuid)?.enabled {
            true => self.check_dependants(uuid, thunderstore),
            false => self.check_dependencies(uuid, thunderstore),
        };

        match dependants {
            Some(dependants) => Ok(ActionResult::Confirm { dependants }),
            None => {
                self.force_toggle_mod(uuid)?;
                Ok(ActionResult::Done)
            }
        }
    }

    pub fn force_toggle_mod(&mut self, uuid: Uuid) -> Result<()> {
        let profile_mod = self.get_mod(uuid)?;

        self.scan_mod(&profile_mod.kind, |dir| {
            let files = WalkDir::new(dir)
                .into_iter()
                .filter_map(Result::ok)
                .filter(|entry| {
                    let file_type = entry.file_type();
                    file_type.is_file() || file_type.is_symlink()
                });

            for file in files {
                let path = file.path();
                let mut new_path = path.to_path_buf();

                if profile_mod.enabled {
                    new_path.add_ext("old");
                } else {
                    // remove all old extensions if multiple got added somehow
                    while let Some("old") = new_path.extension().and_then(|ext| ext.to_str()) {
                        new_path.set_extension("");
                    }
                }

                fs::rename(path, &new_path)?;
            }

            Ok(())
        })?;

        self.get_mod_mut(uuid).unwrap().enabled = !profile_mod.enabled;

        Ok(())
    }

    fn check_dependants(&self, uuid: Uuid, thunderstore: &Thunderstore) -> Option<Vec<Dependant>> {
        let dependants = self
            .dependants(uuid, thunderstore)
            .filter(|profile_mod| {
                // ignore disabled mods and modpacks
                profile_mod.enabled
                    && profile_mod
                        .as_thunderstore()
                        .and_then(|(ts_mod, _)| {
                            ts_mod
                                .id
                                .borrow(thunderstore)
                                .map(|borrowed| !borrowed.package.is_modpack())
                                .ok()
                        })
                        .unwrap_or(true)
            })
            .map_into()
            .collect_vec();

        match dependants.is_empty() {
            true => None,
            false => Some(dependants),
        }
    }

    /// Finds disabled dependencies in the profile.
    fn check_dependencies(
        &self,
        uuid: Uuid,
        thunderstore: &Thunderstore,
    ) -> Option<Vec<Dependant>> {
        let disabled_deps = self
            .get_mod(uuid)
            .ok()?
            .dependencies(thunderstore)
            .filter(|dep| {
                self.get_mod(dep.package.uuid)
                    .is_ok_and(|profile_mod| !profile_mod.enabled)
            })
            .map_into()
            .collect_vec();

        match disabled_deps.is_empty() {
            true => None,
            false => Some(disabled_deps),
        }
    }

    pub fn scan_mod<F>(&self, profile_mod: &ProfileModKind, scan_dir: F) -> Result<()>
    where
        F: Fn(&Path) -> Result<()>,
    {
        let mut path = self.path.join("BepInEx");

        let ident = profile_mod.ident();

        for subdir in self.game.mod_loader.subdirs() {
            path.push(subdir.name);

            match subdir.mode {
                SubdirMode::Separate | SubdirMode::SeparateFlatten => {
                    path.push(ident.full_name());

                    if path.exists() {
                        scan_dir(&path)?;
                    }

                    path.pop();
                }
                SubdirMode::Track => (),
                SubdirMode::None => (),
            }

            path.pop();
        }

        Ok(())
    }

    fn reorder_mod(&mut self, uuid: Uuid, delta: i32) -> Result<()> {
        let index = self
            .mods
            .iter()
            .position(|m| m.uuid() == uuid)
            .context("mod not found in profile")?;

        let target = (index as i32 + delta).clamp(0, self.mods.len() as i32 - 1) as usize;
        let profile_mod = self.mods.remove(index);
        self.mods.insert(target, profile_mod);

        Ok(())
    }
}

pub fn handle_reorder_event(event: tauri::Event, app: &AppHandle) -> Result<()> {
    #[derive(Deserialize)]
    #[serde(rename_all = "camelCase")]
    struct Payload {
        uuid: Uuid,
        delta: i32,
    }

    let Payload { uuid, delta } = serde_json::from_str(event.payload())?;

    let manager = app.state::<Mutex<ModManager>>();
    let prefs = app.state::<Mutex<Prefs>>();

    let mut manager = manager.lock().unwrap();
    let prefs = prefs.lock().unwrap();

    manager.active_profile_mut().reorder_mod(uuid, delta)?;

    manager.save(&prefs)?;

    Ok(())
}

impl ManagedGame {
    pub fn create_profile(&mut self, name: String) -> Result<&mut Profile> {
        ensure!(
            Profile::is_valid_name(&name),
            "profile name '{}' is invalid",
            name
        );

        ensure!(
            !self.profiles.iter().any(|p| p.name == name),
            "profile with name '{}' already exists",
            name
        );

        let mut path = self.path.join("profiles");
        path.push(&name);
        fs::create_dir_all(&path)?;

        self.profiles.push(Profile::new(name, path, self.game));

        let index = self.profiles.len() - 1;
        self.active_profile_index = index;
        Ok(&mut self.profiles[index])
    }

    pub fn delete_profile(&mut self, index: usize, allow_delete_last: bool) -> Result<()> {
        ensure!(
            allow_delete_last || self.profiles.len() > 1,
            "cannot delete last profile"
        );

        let profile = self.profile(index)?;

        fs::remove_dir_all(&profile.path)?;
        self.profiles.remove(index);

        self.active_profile_index = 0;

        Ok(())
    }

    pub fn duplicate_profile(&mut self, duplicate_name: String, index: usize) -> Result<()> {
        self.create_profile(duplicate_name)?;

        let old_profile = self.profile(index)?;
        let new_profile = self.active_profile();

        util::fs::copy_dir(&old_profile.path, &new_profile.path, Overwrite::Yes)?;

        self.active_profile_mut().mods = old_profile.mods.clone();

        Ok(())
    }
}
