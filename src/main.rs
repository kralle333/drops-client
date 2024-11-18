#![windows_subsystem = "windows"]
mod api;
mod blackboard;
mod client_config;
mod errors;
mod handlers;
mod messages;
mod tasks;
mod utils;
mod view_utils;

use crate::client_config::{get_config_dir, ClientConfig, Game, Release, ReleaseState};
use crate::errors::{ConfigError, FetchGamesError, LoginError};
use crate::handlers::client_update::ClientUpdateHandler;
use crate::handlers::download::DownloadMessageHandler;
use crate::handlers::games::GamesMessageHandler;
use crate::handlers::login::LoginMessageHandler;
use crate::handlers::wizard::WizardMessageHandler;
use crate::handlers::MessageHandler;
use crate::messages::Message;
use anyhow::anyhow;
use blackboard::Blackboard;
use env_logger::Env;
use fs2::FileExt;
use futures_util::SinkExt;
use iced::futures::Stream;
use iced::widget::{button, column, row, text, vertical_space};
use iced::{window, Center, Element, Size, Task};
use iced_futures::{stream, Subscription};
use ipc_channel::ipc::{IpcOneShotServer, IpcSender};
use log::{error, info};
use secrecy::SecretString;
use serde::{Deserialize, Serialize};
use std::default::Default;
use std::fs::{File, OpenOptions};
use std::io::{Read, Write};
use std::path::PathBuf;
use std::{env, fs};

#[derive(Default)]
struct DropsClient {
    blackboard: Blackboard,
    downloading: DownloadMessageHandler,
    gaming: GamesMessageHandler,
    wizard: WizardMessageHandler,
    login: LoginMessageHandler,
    requested_game_to_play: Option<String>,
    run_from_args_issue: RunFromArgsIssue,
    client_updating: ClientUpdateHandler,
}

#[derive(Default, Clone, Debug)]
pub enum Screen {
    #[default]
    Empty,
    Wizard,
    ClientUpdateAvailable(self_update::update::Release),
    Login,
    LoggingIn,
    Downloading,
    Main,
    Error(String),
    PlayingGame(String),
}

#[derive(Default)]
enum RunFromArgsIssue {
    #[default]
    NotSet,
    CanPlay(Release),
    Error(String),
    FoundUpdate(Game, Release, Release),
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct SessionToken(String);

impl SessionToken {
    pub fn parse(s: &str) -> SessionToken {
        SessionToken(
            s.split(';')
                .find(|part| part.trim_start().starts_with("id="))
                .unwrap_or("")
                .trim()
                .to_string(),
        )
    }
}

impl DropsClient {
    pub fn new() -> (Self, Task<Message>) {
        let mut client = DropsClient { ..Self::default() };
        let mut args = env::args();
        if args.len() == 2 {
            let _ = args.next();
            let requested_game_to_play = args.next();
            client.requested_game_to_play = requested_game_to_play;
        }

        (
            client,
            Task::batch([Task::perform(
                ClientConfig::load_config(),
                Message::ConfigOpened,
            )]),
        )
    }
    pub fn title(&self) -> String {
        "drops".to_string()
    }
    fn theme(&self) -> iced::Theme {
        iced::Theme::Dark
    }

    pub fn subscription(&self) -> Subscription<Message> {
        Subscription::batch([
            Subscription::run_with_id("ipc", Self::handle_other_clients_opening()),
            self.downloading.subscription(),
        ])
    }

    fn handle_other_clients_opening() -> impl Stream<Item = Message> {
        stream::channel(1, |mut output| async move {
            let mut lock_file = LockFileWithDrop::new();
            loop {
                let result = IpcOneShotServer::new();

                let Ok((server, server_name)) = result else {
                    info!(
                        "failed to create oneshot server: {}",
                        result.err().unwrap().to_string()
                    );
                    return;
                };

                info!("created server");
                let msg = lock_file.write_server_name_lock(&server_name);
                if msg.is_err() {
                    info!(
                        "Failed to write server name {}: {:?}",
                        server_name,
                        msg.err()
                    )
                }

                info!("lets accept the server!");
                let result = tokio::task::spawn_blocking(move || server.accept()).await;
                if let Ok((_, message)) = result.unwrap() {
                    info!("Received message: {}", message);
                    output
                        .send(Message::IpcArgs(message))
                        .await
                        .expect("failed to send args");
                }
            }
        })
    }

    fn have_valid_config(&self) -> bool {
        self.blackboard.have_valid_config()
    }

    pub fn view(&self) -> Element<Message> {
        match &self.blackboard.screen {
            Screen::Empty => column![].into(),
            Screen::Wizard => self.wizard.view(&self.blackboard),
            Screen::Login | Screen::LoggingIn => self.login.view(&self.blackboard),
            Screen::Downloading => self.downloading.view(&self.blackboard),
            Screen::ClientUpdateAvailable(_) => self.client_updating.view(&self.blackboard),
            Screen::PlayingGame(name) => {
                view_utils::centered_container(text(format!("Playing {}", name)).into())
            }
            Screen::Main => {
                match &self.run_from_args_issue {
                    RunFromArgsIssue::Error(_) | RunFromArgsIssue::FoundUpdate(..) => {
                        return Self::display_run_from_args_issue(&self.run_from_args_issue);
                    }
                    _ => {}
                }
                self.gaming.view(&self.blackboard)
            }
            Screen::Error(message) => view_utils::container_with_title(
                "Error".to_string(),
                column![]
                    .push(vertical_space())
                    .push(text(message).size(28).width(300))
                    .push(vertical_space().height(20))
                    .push(button(text("close")).on_press(Message::CloseError))
                    .push(vertical_space()),
            ),
        }
    }

    fn display_run_from_args_issue(issue: &RunFromArgsIssue) -> Element<Message> {
        match issue {
            RunFromArgsIssue::Error(message) => view_utils::centered_container(
                column![]
                    .align_x(Center)
                    .push(text(message).width(300))
                    .push(vertical_space().height(10))
                    .push(button(text("close")).on_press(Message::ClearRequestedGameToPlay))
                    .into(),
            ),
            RunFromArgsIssue::FoundUpdate(game, new_release, installed_release) => {
                view_utils::container_with_title(
                    "Found newer release, update?".to_string(),
                    column![].push(
                        row![]
                            .push(
                                button(text("update"))
                                    .on_press(Message::Download(game.clone(), new_release.clone())),
                            )
                            .push(
                                button(text("play"))
                                    .on_press(Message::Run(installed_release.clone())),
                            )
                            .spacing(10),
                    ),
                )
            }

            _ => column![].into(),
        }
    }

    fn try_run_from_args(&mut self) {
        self.run_from_args_issue = self.handle_args_game_running();
        if let RunFromArgsIssue::CanPlay(release) = &self.run_from_args_issue {
            let game_name_id = self.requested_game_to_play.as_ref().unwrap();
            let games = self.blackboard.config.get_account_games();
            let game = games.iter().find(|x| &x.name_id == game_name_id).unwrap();
            self.blackboard.run_release(game, &release);
            self.run_from_args_issue = RunFromArgsIssue::NotSet;
        }
    }
    fn handle_args_game_running(&mut self) -> RunFromArgsIssue {
        let Some(game_name_id) = &self.requested_game_to_play else {
            return RunFromArgsIssue::NotSet;
        };

        let games = self.blackboard.config.get_account_games();
        let Some(game) = games.iter().find(|x| &x.name_id == game_name_id) else {
            return RunFromArgsIssue::Error(format!(
                "Invalid game {}, but its not installed",
                game_name_id
            ));
        };
        self.blackboard.selected_game = Some(game.clone());
        let channel = match &game.selected_channel {
            None => {
                let release = utils::newest_release_by_state(
                    &game.releases,
                    None,
                    Some(ReleaseState::Installed),
                );
                if release.is_none() {
                    return RunFromArgsIssue::Error(format!(
                        "Found game {}, but its not installed",
                        game.name
                    ));
                }
                release.unwrap().channel_name
            }
            Some(c) => c.to_string(),
        };
        self.blackboard.selected_channel = Some(channel.to_string());
        let release = utils::newest_release_by_state(
            &game.releases,
            Some(&channel),
            Some(ReleaseState::Installed),
        );

        if release.is_none() {
            return RunFromArgsIssue::Error(format!(
                "Found no installed releases for game {}, download one",
                game.name.to_string()
            ));
        }
        let installed_latest_release = release.unwrap();
        match utils::newest_release_by_state(&game.releases, Some(&channel), None) {
            None => {
                // user has the latest release but somehow ended up here...
                RunFromArgsIssue::CanPlay(installed_latest_release)
            }
            Some(latest) if latest.version == installed_latest_release.version => {
                RunFromArgsIssue::CanPlay(installed_latest_release)
            }
            Some(latest) => {
                RunFromArgsIssue::FoundUpdate(game.clone(), latest, installed_latest_release)
            }
        }
    }

    pub fn update(&mut self, message: Message) -> Task<Message> {
        match message {
            Message::IpcArgs(args) => {
                self.requested_game_to_play = Some(args);
                self.try_run_from_args();
            }
            Message::CloseError => self.blackboard.screen = Screen::Main,
            Message::UpdateClient(_) => {
                return self.client_updating.update(message, &mut self.blackboard)
            }
            Message::ClearRequestedGameToPlay => {
                self.run_from_args_issue = RunFromArgsIssue::NotSet;
                self.blackboard.set_initial_screen();
                self.requested_game_to_play = None;
            }
            Message::GoToScreen(screen) => self.blackboard.screen = screen,

            // Games
            Message::Run(_) | Message::SelectGame(_) => {
                return self.gaming.update(message, &mut self.blackboard)
            }

            // Downloading
            Message::CloseDownloadError(_)
            | Message::DownloadProgressing(_)
            | Message::Download(..) => {
                return self.downloading.update(message, &mut self.blackboard)
            }

            // Wizard
            Message::WizardCanReachHostChecked(_)
            | Message::FinishWizard
            | Message::TestDropsUrl
            | Message::DropsUrlChanged(_)
            | Message::SelectGamesDir => return self.wizard.update(message, &mut self.blackboard),

            // Login
            Message::Login
            | Message::ServerChanged(_)
            | Message::LoggedInFinished(_)
            | Message::UsernameChanged(_)
            | Message::PasswordChanged(_) => {
                return self.login.update(message, &mut self.blackboard);
            }
            Message::Logout => self.logout(),
            Message::ConfigOpened(result) => return self.handle_config_open(result),

            Message::FetchGames => {
                return tasks::perform_fetch_games_from_config(&self.blackboard.config)
            }

            Message::GoToInitialScreen => {
                self.blackboard.set_initial_screen();

                if self.blackboard.config.has_session_token() {
                    return tasks::perform_fetch_games_from_config(&self.blackboard.config);
                }
            }
            Message::GamesFetched(Err(e)) => {
                match e {
                    FetchGamesError::APIError(ref inner)
                    | FetchGamesError::Unreachable(ref inner) => {
                        info!("api error: {}", &inner)
                    }
                    FetchGamesError::NotFound => {}
                    FetchGamesError::NeedRelogin | FetchGamesError::BadCredentials => {
                        self.blackboard.screen = Screen::Login;
                        self.blackboard.config.clear_session_token();
                    }
                }
                error!("failed to fetch games! {:?}", e)
            }
            Message::GamesFetched(Ok(games_response)) => {
                self.blackboard
                    .config
                    .sync_and_save(games_response)
                    .expect("Failed to receive games response");
                self.try_run_from_args();
            }

            Message::SelectedChannelChanged(channel_name) => {
                self.blackboard.selected_channel = Some(channel_name);
            }
            Message::SelectedVersionChanged(version) => {
                self.blackboard.selected_version = Some(version);
            }
        }
        Task::none()
    }

    fn handle_config_open(&mut self, result: Result<ClientConfig, ConfigError>) -> Task<Message> {
        self.blackboard.config = result.unwrap_or_else(|e| {
            let error_message = match e {
                ConfigError::DialogClosed => "Dialog closed".to_string(),
                ConfigError::IoError(e) => format!("io error: {}", e).to_string(),
            };
            info!("failed to open config, recreating {}", error_message);
            ClientConfig {
                active_account: Default::default(),
                accounts: vec![],
                is_active: false,
            }
        });
        if self.have_valid_config() {
            let username_in_config = self.blackboard.config.get_username();
            self.login.set_username(&username_in_config);
        }
        self.blackboard.set_initial_screen();

        match utils::look_for_newer_version() {
            Ok(Some(newer_version)) => {
                self.blackboard.screen = Screen::ClientUpdateAvailable(newer_version);
            }
            Ok(_) | Err(_) => {}
        }

        if self.have_valid_config() && self.blackboard.config.has_session_token() {
            return tasks::perform_fetch_games_from_config(&self.blackboard.config);
        }
        Task::none()
    }

    fn logout(&mut self) {
        self.blackboard.selected_game = None;
        self.blackboard.selected_channel = None;
        self.wizard.clear_input();
        self.blackboard.is_playing = false;

        self.login.password_input = SecretString::new("".into());
        self.login.username_input.clear();
        self.login.error_reason = None;

        self.blackboard.screen = Screen::Login;
    }
}

struct LockFileWithDrop {
    pub lock_file: File,
    path: PathBuf,
}

impl LockFileWithDrop {
    fn new() -> Box<Self> {
        let file = OpenOptions::new()
            .read(true)
            .write(true)
            .create(true)
            .open(Self::lock_file_path())
            .expect("Failed to open or create lock file.");

        Box::new(Self {
            lock_file: file,
            path: Self::lock_file_path(),
        })
    }
    fn write_server_name_lock(&mut self, name: &str) -> Result<(), anyhow::Error> {
        info!("Writing server name to file: {}", name);
        self.lock_file.unlock()?;
        self.lock_file.write_all(name.as_bytes())?;
        self.lock_file.try_lock_exclusive()?;
        info!("Success!");
        Ok(())
    }

    fn lock_file_path() -> PathBuf {
        get_config_dir().join("drops.lock")
    }
}

impl Drop for LockFileWithDrop {
    fn drop(&mut self) {
        let unlock_result = self.lock_file.unlock();
        if unlock_result.is_err() {
            error!("Failed to unlock lock file")
        }
        fs::remove_file(&self.path).expect("Failed to delete lock file")
    }
}

fn main() -> Result<(), anyhow::Error> {
    env_logger::Builder::from_env(Env::default().default_filter_or("info")).init();
    {
        /* TODO: Race condition: there is a gap between here
        and the next lock where potentially multiple clients can spawn */
        let mut lock_file = LockFileWithDrop::new();

        if lock_file.lock_file.try_lock_exclusive().is_err() {
            let args: Vec<String> = env::args().skip(1).collect();
            if args.len() > 1 {
                return Err(anyhow!("invalid number of arguments!"));
            }
            // no arguments, just ignore
            if args.len() != 1 {
                return Ok(());
            }
            // let's send this argument to the running instance
            if let Some(arg) = args.iter().nth(0) {
                let mut server_name = String::new();
                let _ = lock_file.lock_file.read_to_string(&mut server_name);
                if let Ok(sender) = IpcSender::<String>::connect(server_name) {
                    sender
                        .send(arg.to_string())
                        .expect("Failed to send arguments.");
                    info!("Arguments sent successfully.");
                } else {
                    info!("Failed to connect to the running instance.");
                }
            }

            return Ok(());
        }
    }

    info!("No running instance found. Starting a new one...");
    let settings = window::settings::Settings {
        size: Size {
            width: 600.0,
            height: 500.0,
        },
        resizable: false,
        decorations: true,
        ..Default::default()
    };

    iced::application(DropsClient::title, DropsClient::update, DropsClient::view)
        .window(settings)
        .theme(DropsClient::theme)
        .subscription(DropsClient::subscription)
        .centered()
        .run_with(DropsClient::new)
        .map_err(|x| anyhow::anyhow!(x))
}
