// Prevents additional console window on Windows in release, DO NOT REMOVE!!
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

use ::log::error;
use anyhow::Context;
use tauri::{
    api::dialog::{blocking::MessageDialogBuilder, MessageDialogKind},
    AppHandle, Manager,
};

#[macro_use]
extern crate lazy_static;

#[cfg(target_os = "linux")]
extern crate webkit2gtk;

#[cfg(target_os = "macos")]
#[macro_use]
extern crate objc;

mod config;
mod games;
mod logger;
mod manager;
mod prefs;
mod thunderstore;
mod util;

#[derive(Debug)]
pub struct NetworkClient(reqwest::Client);

impl NetworkClient {
    fn create() -> Result<Self, reqwest::Error> {
        let client = reqwest::Client::builder()
            .http1_only()
            .user_agent("Kesomannen-gale")
            .build()?;

        Ok(Self(client))
    }
}

fn setup(app: AppHandle) -> anyhow::Result<()> {
    app.manage(NetworkClient::create()?);

    prefs::setup(&app).context("Failed to read settings")?;
    manager::setup(&app).context("Failed to initialize mod manager")?;
    thunderstore::setup(&app);

    Ok(())
}

fn main() {
    // Identifier must match identifier found in tauri.conf.json
    tauri_plugin_deep_link::prepare("com.kesomannen.gale");

    tauri::Builder::default()
        .invoke_handler(tauri::generate_handler![
            logger::open_gale_log,
            logger::log_err,
            thunderstore::commands::query_thunderstore,
            thunderstore::commands::stop_querying_thunderstore,
            thunderstore::commands::get_missing_deps,
            thunderstore::commands::set_thunderstore_token,
            thunderstore::commands::has_thunderstore_token,
            thunderstore::commands::clear_thunderstore_token,
            prefs::commands::get_pref,
            prefs::commands::set_pref,
            prefs::commands::is_first_run,
            manager::commands::get_game_info,
            manager::commands::favorite_game,
            manager::commands::set_active_game,
            manager::commands::get_profile_info,
            manager::commands::set_active_profile,
            manager::commands::is_mod_installed,
            manager::commands::query_profile,
            manager::commands::get_dependants,
            manager::commands::create_profile,
            manager::commands::delete_profile,
            manager::commands::rename_profile,
            manager::commands::duplicate_profile,
            manager::commands::remove_mod,
            manager::commands::force_remove_mods,
            manager::commands::toggle_mod,
            manager::commands::force_toggle_mods,
            manager::commands::reorder_mod,
            manager::commands::set_all_mods_state,
            manager::commands::open_profile_dir,
            manager::commands::open_plugin_dir,
            manager::commands::open_bepinex_log,
            manager::launcher::commands::launch_game,
            manager::downloader::commands::install_mod,
            manager::downloader::commands::cancel_install,
            manager::downloader::commands::clear_download_cache,
            manager::downloader::commands::get_download_size,
            manager::downloader::updater::commands::update_mod,
            manager::downloader::updater::commands::update_all,
            manager::importer::commands::import_data,
            manager::importer::commands::import_code,
            manager::importer::commands::import_file,
            manager::importer::commands::import_local_mod,
            manager::importer::commands::get_r2modman_info,
            manager::importer::commands::import_r2modman,
            manager::exporter::commands::export_code,
            manager::exporter::commands::export_file,
            manager::exporter::commands::export_pack,
            manager::exporter::commands::upload_pack,
            manager::exporter::commands::get_pack_args,
            manager::exporter::commands::set_pack_args,
            manager::exporter::commands::export_dep_string,
            config::commands::get_config_files,
            config::commands::set_tagged_config_entry,
            config::commands::set_untagged_config_entry,
            config::commands::reset_config_entry,
            config::commands::open_config_file,
            config::commands::delete_config_file,
        ])
        .setup(|app| {
            let handle = app.handle();
            logger::setup(&handle).ok();

            if let Err(err) = setup(handle) {
                error!("Failed to launch Gale! {:#}", err);

                MessageDialogBuilder::new("Error while launching Gale!", format!("{:#}", err))
                    .kind(MessageDialogKind::Error)
                    .show();

                return Err(err.into());
            }

            Ok(())
        })
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
