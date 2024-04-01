use std::{
    fs,
    path::{Path, PathBuf},
    process::Command,
    sync::Mutex,
};

use anyhow::{anyhow, bail, Context, Result};
use indexmap::IndexMap;
use serde::{Deserialize, Serialize};
use typeshare::typeshare;
use uuid::Uuid;

use crate::{
    prefs::Prefs,
    thunderstore::{
        self, models::PackageListing, query::{self, QueryModsArgs}, resolve_deps, resolve_deps_all, BorrowedMod, OwnedMod
    },
};

pub mod commands;
pub mod config;
pub mod downloader;
pub mod importer;

pub struct ModManager {
    profiles: Mutex<Vec<Profile>>,
    active_profile_index: Mutex<usize>,
}

#[derive(Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct ManagerSaveData {
    active_profile_index: usize,
}

#[derive(Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct ProfileMod {
    package_uuid: Uuid,
    version_uuid: Uuid,
}

impl ProfileMod {
    fn get<'a>(&self, packages: &'a IndexMap<Uuid, PackageListing>) -> Result<BorrowedMod<'a>> {
        thunderstore::get_mod(&self.package_uuid, &self.version_uuid, packages)
    }
}

struct Profile {
    name: String,
    path: PathBuf,
    mods: Vec<ProfileMod>,
    config: Vec<config::LoadedFile>,
}

impl Profile {
    fn query_mods(
        &self,
        args: QueryModsArgs,
        packages: &IndexMap<Uuid, PackageListing>,
    ) -> Result<Vec<OwnedMod>> {
        let mods = self
            .mods
            .iter()
            .map(|p| p.get(&packages))
            .collect::<Result<Vec<_>, _>>()?;

        Ok(query::query_mods(&args, mods.into_iter()))
    }

    fn get_mod<'a>(&'a self, package_uuid: Uuid) -> Option<&'a ProfileMod> {
        self.mods.iter().find(|p| p.package_uuid == package_uuid)
    }

    fn has_mod(&self, package_uuid: Uuid) -> bool {
        self.get_mod(package_uuid).is_some()
    }

    fn dependants_of<'a>(
        &self,
        package_uuid: Uuid,
        packages: &'a IndexMap<Uuid, PackageListing>,
    ) -> Result<Vec<BorrowedMod<'a>>> {
        let target_mod = self
            .get_mod(package_uuid)
            .context("mod not found in profile")?;

        let target_package = target_mod.get(packages)?.package;

        self.mods
            .iter()
            .filter(|other| other.package_uuid != package_uuid)
            .map(|other| other.get(packages))
            .filter_map(|other| match other {
                Ok(other) => {
                    let deps = resolve_deps_all(&other.version.dependencies, packages)
                        .collect::<Result<Vec<_>>>()
                        .context("failed to resolve dependencies");

                    match deps {
                        Ok(deps) => match deps
                            .into_iter()
                            .any(|dep| dep.package.uuid4 == target_package.uuid4)
                        {
                            true => Some(Ok(other)),
                            false => None,
                        },
                        Err(err) => Some(Err(err)),
                    }
                }
                Err(_) => Some(other),
            }) // filter out packages that do not depend on the target one, while keeping errors
            .collect()
    }

    const GAME_ID: u32 = 1966720;

    fn run_game(&self, config: &Prefs) -> Result<()> {
        let steam_path = config
            .steam_exe_path
            .as_ref()
            .context("steam exe path not set")?;

        let steam_path = resolve_path(&steam_path, "steam")?;

        let mut preloader_path = self.path.join("BepInEx");
        preloader_path.push("core");
        preloader_path.push("BepInEx.Preloader.dll");

        let preloader_path = resolve_path(&preloader_path, "preloader")?;

        Command::new(steam_path)
            .arg("-applaunch")
            .arg(Self::GAME_ID.to_string())
            .arg("--doorstop-enable")
            .arg("true")
            .arg("--doorstop-target")
            .arg(preloader_path)
            .spawn()?;

        return Ok(());

        fn resolve_path<'a>(path: &'a PathBuf, name: &'static str) -> Result<&'a str> {
            let str = path.to_str();
            if !path.try_exists()? || str.is_none() {
                bail!("{} path could not be resolved", name);
            }
            Ok(str.unwrap())
        }
    }
}

#[typeshare]
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Dependant {
    pub name: String,
    pub uuid: Uuid,
}

#[typeshare]
#[derive(Serialize)]
#[serde(rename_all = "camelCase", tag = "type", content = "data")]
pub enum RemoveModResponse {
    Removed,
    HasDependants(Vec<Dependant>),
}

impl Profile {
    fn remove_mod(
        &mut self,
        package_uuid: Uuid,
        packages: &IndexMap<Uuid, PackageListing>,
    ) -> Result<RemoveModResponse> {
        let dependants = self.dependants_of(package_uuid, packages)?;

        if !dependants.is_empty() {
            let response = dependants
                .iter()
                .map(|m| Dependant {
                    name: m.package.name.clone(),
                    uuid: m.package.uuid4,
                })
                .collect();

            return Ok(RemoveModResponse::HasDependants(response));
        }

        self.force_remove_mod(package_uuid, packages)?;

        Ok(RemoveModResponse::Removed)
    }

    fn force_remove_mod(
        &mut self,
        package_uuid: Uuid,
        packages: &IndexMap<Uuid, PackageListing>,
    ) -> Result<()> {
        let index = self
            .mods
            .iter()
            .position(|m| m.package_uuid == package_uuid)
            .context("mod not found in profile")?;

        let package = thunderstore::get_package(&package_uuid, packages)?;

        let mut path = self.path.join("BepInEx");
        for dir in ["core", "patchers", "plugins"].iter() {
            path.push(dir);
            path.push(&package.full_name);

            if path.try_exists().unwrap_or(false) {
                fs::remove_dir_all(&path).context("failed to remove mod directory")?;
            }

            path.pop();
            path.pop();
        }

        self.mods.remove(index);

        Ok(())
    }
}

impl ModManager {
    pub fn init(options: &Prefs) -> Result<Self> {
        println!("initiating manager");

        let save_path = options.data_path.join("manager.json");
        let save_data = match save_path.try_exists()? {
            true => {
                let json = fs::read_to_string(save_path)?;
                let data =
                    serde_json::from_str(&json).context("failed to parse manager save data")?;
                data
            }
            false => ManagerSaveData {
                active_profile_index: 0,
            },
        };

        let profiles_path = options.data_path.join("profiles");
        fs::create_dir_all(&profiles_path)?;

        let mut profiles = Vec::new();
        for entry in profiles_path.read_dir()? {
            let path = entry?.path();
            if path.is_dir() {
                profiles.push(
                    load_profile(&path)
                        .with_context(|| format!("failed to load profile at {:?}", &path))?,
                );
            }
        }

        println!(
            "loaded profiles: {:?}",
            profiles.iter().map(|p| &p.name).collect::<Vec<_>>()
        );

        let is_empty = profiles.is_empty();

        let manager = Self {
            profiles: Mutex::new(profiles),
            active_profile_index: Mutex::new(save_data.active_profile_index),
        };

        if is_empty {
            manager.create_profile("Default".to_string(), options)?;
            manager.save(options)?;
        }

        Ok(manager)
    }

    fn save(&self, prefs: &Prefs) -> Result<()> {
        let manager_save_data = ManagerSaveData {
            active_profile_index: *self.active_profile_index.lock().unwrap(),
        };

        let json = serde_json::to_string_pretty(&manager_save_data)?;
        let save_path = prefs.data_path.join("manager.json");
        fs::write(save_path, json)?;

        let profiles = self.profiles.lock().unwrap();
        let mut path = prefs.data_path.join("profiles");
        for profile in profiles.iter() {
            let json = serde_json::to_string_pretty(&profile.mods)?;
            path.push(&profile.name);
            path.push("manifest.json");

            fs::write(&path, json)?;

            path.pop();
            path.pop();
        }

        Ok(())
    }

    fn create_profile(&self, name: String, options: &Prefs) -> Result<usize> {
        let mut profiles = self.profiles.lock().unwrap();
        if profiles.iter().any(|p| p.name == name) {
            return Err(anyhow!("profile with name {} already exists", name));
        }

        let mut path = options.data_path.join("profiles");
        path.push(&name);
        fs::create_dir_all(&path)?;

        let profile = Profile {
            name,
            path,
            mods: Vec::new(),
            config: Vec::new(),
        };
        profiles.push(profile);

        Ok(profiles.len() - 1)
    }

    fn delete_profile(&self, index: usize) -> Result<()> {
        let mut profiles = self.profiles.lock().unwrap();
        let profile = profiles.get(index).context("profile not found")?;

        if profiles.len() == 1 {
            return Err(anyhow!("cannot delete last profile"));
        }

        let mut active_profile_index = self.active_profile_index.lock().unwrap();
        if *active_profile_index == index {
            *active_profile_index = 0;
        }

        fs::remove_dir_all(&profile.path)?;
        profiles.remove(index);

        Ok(())
    }

    fn set_active_profile(&self, index: usize) -> Result<()> {
        let mut active_profile = self.active_profile_index.lock().unwrap();
        let profiles = self.profiles.lock().unwrap();

        if index >= profiles.len() {
            return Err(anyhow!("profile index {} out of bounds", index));
        }

        *active_profile = index;

        Ok(())
    }
}

fn load_profile(path: &Path) -> Result<Profile, anyhow::Error> {
    let name = path.file_name().unwrap().to_string_lossy().to_string();
    let mods = fs::read_to_string(path.join("manifest.json"))
        .context("failed to read profile manifest")?;

    let mods: Vec<ProfileMod> =
        serde_json::from_str(&mods).context("failed to parse profile manifest")?;

    let config = config::load_config(&path).collect();

    Ok(Profile {
        name,
        path: path.to_owned(),
        mods,
        config,
    })
}

fn get_active_profile<'a>(
    profiles: &'a mut Vec<Profile>,
    manager: &ModManager,
) -> Result<&'a mut Profile> {
    let active_profile_index = *manager.active_profile_index.lock().unwrap();
    profiles
        .get_mut(active_profile_index)
        .context("active profile out of range")
}
