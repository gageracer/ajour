use crate::{addon::Addon, error::ClientError, Result};
use async_std::{
    fs::{create_dir_all, File},
    io::copy,
    prelude::*,
};
use isahc::config::RedirectPolicy;
use isahc::http::header::CONTENT_LENGTH;
use isahc::prelude::*;
use serde::Serialize;
use std::path::PathBuf;

/// Generic request function.
pub async fn request_async<T: ToString>(
    shared_client: &HttpClient,
    url: T,
    headers: Vec<(&str, &str)>,
    timeout: Option<u64>,
) -> Result<Response<isahc::Body>> {
    // Sometimes a download url has a space.
    let url = url.to_string().replace(" ", "%20");

    let mut request = Request::builder().uri(url);

    for (name, value) in headers {
        request = request.header(name, value);
    }

    if let Some(timeout) = timeout {
        request = request.timeout(std::time::Duration::from_secs(timeout));
    }

    Ok(shared_client.send_async(request.body(())?).await?)
}

// Generic function for posting Json data
pub async fn post_json_async<T: ToString, D: Serialize>(
    url: T,
    data: D,
    headers: Vec<(&str, &str)>,
    timeout: Option<u64>,
) -> Result<Response<isahc::Body>> {
    let mut request = Request::builder()
        .method("POST")
        .uri(url.to_string())
        .header("content-type", "application/json");

    for (name, value) in headers {
        request = request.header(name, value);
    }

    if let Some(timeout) = timeout {
        request = request.timeout(std::time::Duration::from_secs(timeout));
    }

    Ok(request
        .body(serde_json::to_vec(&data)?)?
        .send_async()
        .await?)
}

/// Function to download a zip archive for a `Addon`.
/// Note: Addon needs to have a `remote_url` to the file.
pub async fn download_addon(
    shared_client: &HttpClient,
    addon: &Addon,
    to_directory: &PathBuf,
) -> Result<()> {
    if let Some(package) = addon.relevant_release_package() {
        log::debug!(
            "downloading remote version {} for {}",
            package.version,
            &addon.id
        );
        let mut resp =
            request_async(shared_client, package.download_url.clone(), vec![], None).await?;
        let body = resp.body_mut();

        if !to_directory.exists() {
            create_dir_all(to_directory).await?;
        }

        let zip_path = to_directory.join(&addon.id);
        let mut buffer = [0; 8000]; // 8KB
        let mut file = File::create(&zip_path).await?;

        loop {
            match body.read(&mut buffer).await {
                Ok(0) => {
                    break;
                }
                Ok(x) => {
                    file.write_all(&buffer[0..x])
                        .await
                        .expect("TODO: error handling");
                }
                Err(e) => {
                    println!("error: {:?}", e);
                    break;
                }
            }
        }
    }

    Ok(())
}

/// Download a file from the internet
pub async fn download_file<T: ToString>(url: T, dest_file: &PathBuf) -> Result<()> {
    let url = url.to_string();

    log::debug!("downloading file from {}", &url);

    let client = HttpClient::builder()
        .redirect_policy(RedirectPolicy::Follow)
        .build()?;

    let resp = request_async(
        &client,
        &url,
        vec![("ACCEPT", "application/octet-stream")],
        None,
    )
    .await?;
    let (parts, body) = resp.into_parts();

    // If response length doesn't equal content length, full file wasn't downloaded
    // so error out
    {
        let content_length = parts
            .headers
            .get(CONTENT_LENGTH)
            .map(|v| v.to_str().unwrap_or_default())
            .unwrap_or_default()
            .parse::<u64>()
            .unwrap_or_default();

        let body_length = body.len().unwrap_or_default();

        if body_length != content_length {
            return Err(ClientError::Custom(
                "Download failed, body len doesn't match content len".to_string(),
            ));
        }
    }

    let file = File::create(&dest_file).await?;

    copy(body, file).await?;

    log::debug!("file saved as {:?}", &dest_file);

    Ok(())
}
