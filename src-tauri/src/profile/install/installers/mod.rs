use std::{
    io::Cursor,
    path::{Path, PathBuf},
};

use anyhow::Result;
use zip::ZipArchive;

use super::fs::{ConflictResolution, FileInstallMethod};
use crate::profile::{Profile, ProfileMod};

mod bepinex;
mod extract;
mod gd_weave;
mod shimloader;
mod subdir;

pub use self::{
    bepinex::BepinexInstaller,
    extract::ExtractInstaller,
    gd_weave::GDWeaveModInstaller,
    shimloader::ShimloaderInstaller,
    subdir::{Subdir, SubdirInstaller},
};

pub type PackageZip = ZipArchive<Cursor<Vec<u8>>>;

pub trait PackageInstaller {
    /// Extracts a mod archive to `dest`.
    fn extract(&mut self, archive: PackageZip, package_name: &str, dest: PathBuf) -> Result<()>;

    /// Installs a package from `src` to a profile.
    fn install(&mut self, src: &Path, _package_name: &str, profile: &Profile) -> Result<()> {
        super::fs::install(src, profile, |_, _| {
            Ok((FileInstallMethod::Link, ConflictResolution::Overwrite))
        })
    }

    fn toggle(&mut self, enabled: bool, profile_mod: &ProfileMod, profile: &Profile) -> Result<()>;
    fn uninstall(&mut self, profile_mod: &ProfileMod, profile: &Profile) -> Result<()>;

    fn mod_dir<'a>(&'a self, profile_mod: &ProfileMod, profile: &Profile) -> Option<&'a Path> {
        None
    }
}
