use crate::errors::LoginError::{APIError, BadCredentials, MissingSessionToken};
use crate::errors::{FetchGamesError, LoginError};
use crate::{utils, SessionToken};
use drops_messages::requests::{GetGamesRequest, GetGamesResponse};
use reqwest::redirect::Policy;
use reqwest::{Client, ClientBuilder, StatusCode};
use std::error;
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
    pub channel_name: String,
}

pub(crate) fn build_client() -> Client {
    ClientBuilder::new()
        .redirect(Policy::none())
        .build()
        .unwrap()
}

pub async fn login(
    drops_url: String,
    username: String,
    password: String,
) -> Result<SessionToken, LoginError> {
    let client = build_client();
    let resp = client
        .post(format!("{}/login", drops_url))
        .timeout(Duration::from_secs(5))
        .basic_auth(username, Some(password))
        .send()
        .await?;

    match resp.status() {
        StatusCode::OK => {
            let cookie = match resp.headers().get("set-cookie") {
                Some(session) if session.to_str().is_ok() => session.to_str().unwrap(),
                None | Some(_) => return Err(MissingSessionToken),
            };
            Ok(SessionToken::parse(cookie))
        }
        StatusCode::UNAUTHORIZED => Err(BadCredentials),
        _ => Err(APIError),
    }
}

pub async fn fetch_games(
    url: String,
    session_token: SessionToken,
) -> Result<GetGamesResponse, FetchGamesError> {
    let req = GetGamesRequest {
        platform: Some(utils::default_platform().into()),
    };

    let client = build_client();
    let url = format!("{}/games", url);
    let resp = client
        .get(url)
        .json(&req)
        .header("Cookie", session_token.0)
        .timeout(Duration::from_secs(5))
        .send()
        .await?;

    if resp.status().is_redirection() {
        return Err(FetchGamesError::NeedRelogin);
    }

    let resp: GetGamesResponse = resp.json().await?;

    Ok(resp)
}

pub fn unzip_file(
    archive: &mut ZipArchive<Cursor<Vec<u8>>>,
    output_dir: &str,
) -> Result<(), Box<dyn error::Error>> {
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
                fs::set_permissions(&outpath, fs::Permissions::from_mode(mode))?;
            }
        }
    }

    Ok(())
}

pub async fn can_reach_host(url: String) -> Result<(), String> {
    match build_client().get(url).send().await {
        Ok(x) => {
            if x.status() == 200 {
                let page = x.text().await.unwrap();
                match page.contains("💧") {
                    true => Ok(()),
                    false => Err("not a drops server".to_string()),
                }
            } else {
                Err(format!("failed with err: {}", x.status()).to_string())
            }
        }
        Err(e) => Err(e.to_string()),
    }
}
