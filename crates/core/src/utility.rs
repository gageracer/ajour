use crate::error::ClientError;
use crate::network::{download_file, request_async};
use crate::Result;

use isahc::prelude::*;
use regex::Regex;
use serde::Deserialize;

use std::ffi::OsStr;
use std::path::PathBuf;

/// Takes a `&str` and strips any non-digit.
/// This is used to unify and compare addon versions:
///
/// A string looking like 213r323 would return 213323.
/// A string looking like Rematch_4_10_15.zip would return 41015.
pub fn strip_non_digits(string: &str) -> Option<String> {
    let re = Regex::new(r"[\D]").unwrap();
    let stripped = re.replace_all(string, "").to_string();
    Some(stripped)
}

#[derive(Debug, Deserialize, Clone)]
pub struct Release {
    pub tag_name: String,
    pub assets: Vec<ReleaseAsset>,
}

#[derive(Debug, Deserialize, Clone)]
pub struct ReleaseAsset {
    pub name: String,
    #[serde(rename = "browser_download_url")]
    pub download_url: String,
}

pub async fn get_latest_release() -> Option<Release> {
    log::debug!("checking for application update");

    let client = HttpClient::new().ok()?;

    let mut resp = request_async(
        &client,
        "https://api.github.com/repos/tarkah/ajour_self_update_test/releases/latest",
        vec![],
        None,
    )
    .await
    .ok()?;

    Some(resp.json().ok()?)
}

/// Downloads the latest release file that matches `bin_name` and saves it as
/// `tmp_bin_name`. Will return the temp file as pathbuf.
pub async fn download_update_to_temp_file(bin_name: String, release: Release) -> Result<PathBuf> {
    let asset = release
        .assets
        .iter()
        .find(|a| a.name == bin_name)
        .cloned()
        .ok_or_else(|| {
            ClientError::Custom(format!("No new release binary available for {}", bin_name))
        })?;

    let current_bin_path = std::env::current_exe()?;

    let new_bin_path = current_bin_path
        .parent()
        .unwrap()
        .join(&format!("tmp_{}", bin_name));

    download_file(&asset.download_url, &new_bin_path).await?;

    // Make executable
    #[cfg(not(windows))]
    {
        use std::os::unix::fs::PermissionsExt;

        let mut permissions = fs::metadata(&new_bin_path).await?.permissions();
        permissions.set_mode(0o755);
        fs::set_permissions(&new_bin_path, permissions).await?;
    }

    Ok(new_bin_path)
}

/// Logic to help pick the right World of Warcraft folder. We want the root folder.
pub fn wow_path_resolution(path: Option<PathBuf>) -> Option<PathBuf> {
    if let Some(path) = path {
        // Known folders in World of Warcraft dir
        let known_folders = ["_retail_", "_classic_", "_ptr_"];

        // If chosen path has any of the known Wow folders, we have the right one.
        for folder in known_folders.iter() {
            if path.join(folder).exists() {
                return Some(path);
            }
        }

        // Iterate ancestors. If we find any of the known folders we can guess the root.
        for ancestor in path.as_path().ancestors() {
            if let Some(file_name) = ancestor.file_name() {
                for folder in known_folders.iter() {
                    if file_name == OsStr::new(folder) {
                        return ancestor.parent().map(|p| p.to_path_buf());
                    }
                }
            }
        }
    }

    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_wow_path_resolution() {
        let classic_addon_path =
            PathBuf::from(r"/Applications/World of Warcraft/_classic_/Interface/Addons");
        let retail_addon_path =
            PathBuf::from(r"/Applications/World of Warcraft/_retail_/Interface/Addons");
        let retail_interface_path =
            PathBuf::from(r"/Applications/World of Warcraft/_retail_/Interface");
        let classic_interface_path =
            PathBuf::from(r"/Applications/World of Warcraft/_classic_/Interface");
        let classic_alternate_path = PathBuf::from(r"/Applications/Wow/_classic_");

        let root_alternate_path = PathBuf::from(r"/Applications/Wow");
        let root_path = PathBuf::from(r"/Applications/World of Warcraft");

        assert_eq!(
            root_path.eq(&wow_path_resolution(Some(classic_addon_path)).unwrap()),
            true
        );
        assert_eq!(
            root_path.eq(&wow_path_resolution(Some(retail_addon_path)).unwrap()),
            true
        );
        assert_eq!(
            root_path.eq(&wow_path_resolution(Some(retail_interface_path)).unwrap()),
            true
        );
        assert_eq!(
            root_path.eq(&wow_path_resolution(Some(classic_interface_path)).unwrap()),
            true
        );
        assert_eq!(
            root_alternate_path.eq(&wow_path_resolution(Some(classic_alternate_path)).unwrap()),
            true
        );
    }
}
