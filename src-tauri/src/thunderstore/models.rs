use std::hash::Hash;

use serde::{Deserialize, Serialize};
use uuid::Uuid;

#[derive(Serialize, Deserialize, Debug, Clone, Eq)]
pub struct PackageListing {
    pub categories: Vec<String>,
    pub date_created: String,
    pub date_updated: String,
    pub donation_link: Option<String>,
    pub full_name: String,
    pub has_nsfw_content: bool,
    pub is_deprecated: bool,
    pub is_pinned: bool,
    pub name: String,
    pub owner: String,
    pub package_url: String,
    pub rating_score: u32,
    pub uuid4: Uuid,
    pub versions: Vec<PackageVersion>,
}

impl PackageListing {
    pub fn get_version(&self, uuid: &Uuid) -> Option<&PackageVersion> {
        self.versions.iter().find(|v| v.uuid4 == *uuid)
    }

    pub fn get_version_with_num(&self, version: &str) -> Option<&PackageVersion> {
        self.versions.iter().find(|v| v.version_number == version)
    }

    pub fn total_downloads(&self) -> u32 {
        self.versions.iter().map(|v| v.downloads).sum()
    }
}

impl PartialEq for PackageListing {
    fn eq(&self, other: &Self) -> bool {
        self.uuid4 == other.uuid4
    }
}

impl Hash for PackageListing {
    fn hash<H: std::hash::Hasher>(&self, state: &mut H) {
        self.uuid4.hash(state);
    }
}

#[derive(Serialize, Deserialize, Debug, Clone, Eq)]
pub struct PackageVersion {
    pub date_created: String,
    pub dependencies: Vec<String>,
    pub description: String,
    pub download_url: String,
    pub downloads: u32,
    pub file_size: u32,
    pub full_name: String,
    pub icon: String,
    pub is_active: bool,
    pub name: String,
    pub uuid4: Uuid,
    pub version_number: String,
    pub website_url: String,
}

impl PartialEq for PackageVersion {
    fn eq(&self, other: &Self) -> bool {
        self.uuid4 == other.uuid4
    }
}

impl Hash for PackageVersion {
    fn hash<H: std::hash::Hasher>(&self, state: &mut H) {
        self.uuid4.hash(state);
    }
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct LegacyProfileCreateResponse {
    pub key: Uuid,
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct PackageManifest {
    pub name: String,
    pub description: String,
    pub version_number: String,
    pub dependencies: Vec<String>,
    pub website_url: String,
    pub installers: Option<Vec<PackageInstaller>>,
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct PackageInstaller {
    pub identifier: String,
}
