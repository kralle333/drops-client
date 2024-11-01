use crate::errors::ConfigError;
use anyhow::{anyhow, Error};
use chrono::{DateTime, Utc};
use directories::ProjectDirs;
use drops_messages::requests::{GameInfoResponse, GetGamesResponse, ReleaseInfoResponse};
use serde_json::from_str;
use std::collections::HashMap;
use std::fs::File;
use std::io::Write;
use std::path::PathBuf;
use std::sync::Arc;

pub fn get_config_dir() -> PathBuf {
    ProjectDirs::from("com", "Drops", "Drops Client")
        .unwrap()
        .config_local_dir()
        .into()
}

pub fn get_config_file_path() -> PathBuf {
    get_config_dir().join("config.yaml")
}

pub fn ensure_path() {
    let path = get_config_dir();

    if !path.exists() {
        std::fs::create_dir_all(&path).expect("failed to create config dir");
    }
}

#[derive(serde::Deserialize, serde::Serialize, Debug, Clone)]
pub struct ClientConfig {
    pub games_dir: String,
    pub username: String,
    pub drops_url: String,
    pub games: Vec<Game>,
}

#[derive(serde::Deserialize, serde::Serialize, Debug, Clone)]
pub struct Game {
    pub name: String,
    pub name_id: String,
    pub description: String,
    pub author: String,
    pub orphaned: bool,
    pub selected_channel: Option<String>,
    pub releases: Vec<Release>,
}

#[derive(serde::Deserialize, serde::Serialize, PartialEq, Debug, Clone)]
pub enum ReleaseState {
    NotInstalled,
    Installed,
    NeedsUpdate,
}

#[derive(serde::Deserialize, serde::Serialize, Debug, Clone)]
pub struct Release {
    pub channel_name: String,
    pub version: String,
    pub state: ReleaseState,
    pub release_date: DateTime<Utc>,
    pub executable_path: String,
}

impl ClientConfig {
    pub(crate) fn save(&self) -> Result<(), Error> {
        ensure_path();
        let as_str = serde_json::to_string(self)?;

        let mut file = File::create(get_config_file_path())?;
        file.write_all(&as_str.as_bytes())?;

        Ok(())
    }

    fn create_new_release(r: &ReleaseInfoResponse) -> Release {
        Release {
            channel_name: r.channel.to_string(),
            version: r.version.to_string(),
            state: ReleaseState::NotInstalled,
            release_date: r.release_date,
            executable_path: r.executable_path.to_string(),
        }
    }

    fn add_new_game(&mut self, game_info: GameInfoResponse) -> Result<(), Error> {
        let releases: Vec<Release> = game_info
            .releases
            .iter()
            .map(|r| Self::create_new_release(r))
            .collect();
        let stored_game = Game {
            name: game_info.name,
            name_id: game_info.name_id,
            description: game_info.description,
            author: game_info.author,
            releases,
            orphaned: false,
            selected_channel: game_info.default_channel,
        };

        self.games.push(stored_game);
        self.save()
    }

    fn patch_existing_game(
        &mut self,
        existing_game: Game,
        game_info: GameInfoResponse,
    ) -> Result<(), Error> {
        let mut patched_game = Game {
            name: game_info.name,
            name_id: existing_game.name_id,
            description: game_info.description.to_string(),
            author: game_info.author.to_string(),
            orphaned: false,
            selected_channel: None,
            releases: vec![],
        };

        let new: Vec<_> = game_info
            .releases
            .iter()
            .filter(|x| {
                existing_game
                    .releases
                    .iter()
                    .find(|y| y.version == x.version)
                    .is_none()
            })
            .collect();

        patched_game.releases = new
            .iter()
            .into_iter()
            .map(|x| Self::create_new_release(x))
            .collect();

        patched_game.releases.extend(existing_game.releases);

        match self
            .games
            .iter_mut()
            .find(|x| &x.name_id == &patched_game.name_id)
        {
            None => {
                return Err(anyhow!(
                    "Failed to find patch game with name_id: {}",
                    &patched_game.name_id
                ));
            }
            Some(game) => {
                *game = patched_game;
            }
        }
        self.save()
    }

    pub fn update_install_state(
        &mut self,
        game_name_id: &str,
        version: &str,
        channel_name: &str,
        state: ReleaseState,
    ) -> Result<(), Error> {
        match self.games.iter_mut().find(|x| &x.name_id == &game_name_id) {
            None => {
                return Err(anyhow!(
                    "Failed to find game with name_id: {}",
                    &game_name_id
                ));
            }
            Some(game) => match game
                .releases
                .iter_mut()
                .find(|y| &y.version == &version && &y.channel_name == channel_name)
            {
                None => {
                    return Err(anyhow!(
                        "Failed to find release {} {}",
                        &version,
                        channel_name
                    ));
                }
                Some(release) => {
                    release.state = state;
                }
            },
        }
        self.save()
    }

    pub fn sync_and_save(&mut self, game_info_response: GetGamesResponse) -> Result<(), Error> {
        let mut existing_game_map: HashMap<String, Game> = self
            .games
            .iter()
            .map(|x| (x.name_id.to_string(), x.clone().to_owned()))
            .collect();

        for x in game_info_response.games {
            let stored_game = existing_game_map.remove(&x.name_id);
            match stored_game {
                None => self.add_new_game(x)?,
                Some(existing_game) => self.patch_existing_game(existing_game, x)?,
            }
        }

        let orphaned_games: Vec<String> = existing_game_map
            .into_iter()
            .map(|(_, game)| game.name_id)
            .collect();

        if orphaned_games.len() == 0 {
            return Ok(());
        }

        self.games
            .iter_mut()
            .filter(|x| orphaned_games.contains(&x.name_id.to_string()))
            .into_iter()
            .for_each(|x| x.orphaned = true);

        self.save()
    }

    pub async fn load_config() -> Result<ClientConfig, ConfigError> {
        let path = get_config_file_path();
        ensure_path();

        let contents = tokio::fs::read_to_string(&path)
            .await
            .map(Arc::new)
            .map_err(|error| ConfigError::IoError(error.kind()))?;

        let config: ClientConfig = match from_str(&contents) {
            Ok(c) => c,
            Err(_) => return Err(ConfigError::DialogClosed),
        };

        Ok(config)
    }
}
