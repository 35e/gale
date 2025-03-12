use eyre::{OptionExt, Result};
use log::info;
use tauri::AppHandle;

use super::{InstallOptions, ModInstall};
use crate::{
    logger,
    state::ManagerExt,
    thunderstore::{ModId, Thunderstore},
};

pub fn handle(url: &str, app: &AppHandle) {
    let mod_id = {
        let thunderstore = app.lock_thunderstore();

        match resolve_url(url, &thunderstore) {
            Ok(mod_id) => mod_id,
            Err(err) => {
                logger::log_webview_err("Failed to resolve deep link", err, app);
                return;
            }
        }
    };

    let handle = app.clone();
    tauri::async_runtime::spawn(async move {
        super::install_with_deps(
            vec![ModInstall::new(mod_id)],
            InstallOptions::default(),
            false,
            &handle,
        )
        .await
        .unwrap_or_else(|err| {
            logger::log_webview_err("Failed to install mod from deep link", err, &handle);
        });
    });
}

fn resolve_url(url: &str, thunderstore: &Thunderstore) -> Result<ModId> {
    let (owner, name, version) = url
        .strip_prefix("ror2mm://v1/install/thunderstore.io/")
        .and_then(|path| {
            let mut split = path.split('/');

            Some((split.next()?, split.next()?, split.next()?))
        })
        .ok_or_eyre("invalid deep link url")?;

    let borrow = thunderstore.find_mod(owner, name, version)?;

    info!("installing {} from deep link", borrow.ident());

    Ok(borrow.into())
}
