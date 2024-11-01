use crate::client_config::ClientConfig;
use crate::default_platform;
use crate::errors::DownloadGameError::IoError;
use crate::errors::LoginError::{APIError, BadCredentials};
use crate::errors::{DownloadGameError, FetchGamesError, LoginError};
use drops_messages::requests::{GetGamesRequest, GetGamesResponse};
use futures_util::StreamExt;
use log::error;
use reqwest::{Client, StatusCode};
use std::fs;
use std::fs::File;
use std::io::Cursor;
use std::path::Path;
use std::time::Duration;
use zip::ZipArchive;

#[derive(Debug, Clone)]
pub struct InstalledRelease {
    pub game_name_id: String,
    pub version: String,
    pub channel: String,
}

pub async fn login(
    client: Client,
    config: ClientConfig,
    username: String,
    password: String,
) -> Result<Client, LoginError> {
    let resp = client
        .post(format!("{}/login", config.drops_url))
        .timeout(Duration::from_secs(5))
        .basic_auth(username, Some(password))
        .send()
        .await?;

    match resp.status() {
        StatusCode::OK => Ok(client),
        StatusCode::UNAUTHORIZED => Err(BadCredentials),
        _ => Err(APIError),
    }
}

pub async fn fetch_games(
    mut config: ClientConfig,
    client: Client,
) -> Result<ClientConfig, FetchGamesError> {
    let req = GetGamesRequest {
        platform: Some(default_platform().into()),
    };

    let resp: GetGamesResponse = client
        .get(format!("{}/games", config.drops_url))
        .timeout(Duration::from_secs(5))
        .json(&req)
        .send()
        .await?
        .json()
        .await?;

    config
        .sync_and_save(resp)
        .map_err(|_| FetchGamesError::ConfigSavingFailed)?;

    Ok(config)
}

pub fn unzip_file(
    archive: &mut ZipArchive<Cursor<Vec<u8>>>,
    output_dir: &str,
) -> Result<(), Box<dyn std::error::Error>> {
    // Iterate through the zip entries
    for i in 0..archive.len() {
        let mut file = archive.by_index(i)?;
        let outpath = Path::new(output_dir).join(file.name());

        println!("Extracting file: {}", outpath.display());

        // Check if the file is a directory or a file
        if file.name().ends_with('/') {
            fs::create_dir_all(&outpath)?;
        } else {
            // Create the directory if it doesn't exist
            if let Some(parent) = outpath.parent() {
                if !parent.exists() {
                    fs::create_dir_all(parent)?;
                }
            }
            // Extract the file
            let mut outfile = File::create(&outpath)?;
            std::io::copy(&mut file, &mut outfile)?;
        }

        // If the file has a Unix mode (like permissions), set it
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            if let Some(mode) = file.unix_mode() {
                std::fs::set_permissions(&outpath, std::fs::Permissions::from_mode(mode))?;
            }
        }
    }

    Ok(())
}
pub async fn download_game(
    client: Client,
    config: ClientConfig,
    game_name_id: String,
    version: String,
    channel: String,
) -> Result<InstalledRelease, DownloadGameError> {
    let response = client
        .get(format!(
            "{}/releases/{}/{}/{}/{}",
            config.drops_url,
            game_name_id,
            default_platform(),
            channel,
            version
        ))
        .send()
        .await?;

    let stream = response.bytes_stream();
    let mut zip_data = Vec::new();
    tokio::pin!(stream); // Pin the stream for iteration
    while let Some(chunk) = stream.next().await {
        zip_data.extend_from_slice(&chunk?);
    }

    let reader = Cursor::new(zip_data);
    let mut zip = ZipArchive::new(reader).map_err(|_| IoError)?;

    let output_dir = Path::new(&config.games_dir)
        .join(&game_name_id)
        .join(&channel)
        .join(&version);
    fs::create_dir_all(&output_dir).expect("failed creating unzip folder");
    unzip_file(&mut zip, output_dir.as_path().to_str().unwrap()).map_err(|e| {
        error!("Failed to unzip file: {}", e);
        IoError
    })?;

    Ok(InstalledRelease {
        game_name_id,
        version,
        channel,
    })
}

pub async fn can_reach_host(url: String) -> bool {
    let client = Client::new();
    match client.get(url).timeout(Duration::from_secs(5)).send().await {
        Ok(_) => true,
        Err(_) => false,
    }
}
