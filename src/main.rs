#![windows_subsystem = "windows"]
mod api;
mod blackboard;
mod client_config;
mod downloading;
mod errors;
mod handlers;
mod messages;
mod tasks;
mod view_utils;

use crate::client_config::{ClientConfig, Game, Release, ReleaseState};
use crate::downloading::DownloadError;
use crate::downloading::DownloadState;
use crate::errors::{ConfigError, FetchGamesError, LoginError};
use crate::handlers::download::DownloadMessageHandler;
use crate::handlers::games::GamesMessageHandler;
use crate::handlers::login::LoginMessageHandler;
use crate::handlers::wizard::WizardMessageHandler;
use crate::handlers::MessageHandler;
use crate::messages::Message;
use crate::view_utils::container_with_top_bar_and_side_view;
use anyhow::{anyhow, Context};
use blackboard::Blackboard;
use env_logger::Env;
use iced::widget::{
    button, column, container, pick_list, progress_bar, row, scrollable, text, vertical_space,
    Column, Container,
};
use iced::{window, Center, Color, Element, Fill, Size, Task};
use iced_futures::Subscription;
use log::error;
use secrecy::SecretString;
use self_update::{cargo_crate_version, self_replace, version};
use serde::{Deserialize, Serialize};
use std::collections::HashSet;
use std::default::Default;
use std::env;

#[derive(Default)]
struct DropsClient {
    blackboard: Blackboard,
    downloading: DownloadMessageHandler,
    gaming: GamesMessageHandler,
    wizard: WizardMessageHandler,
    login: LoginMessageHandler,
    requested_game_to_play: Option<String>,
    run_from_args_issue: RunFromArgsIssue,
    update_client_error: String,
    is_updating_client: bool,
}

#[derive(Default, Clone, Debug)]
pub enum Screen {
    #[default]
    Wizard,
    ClientUpdateAvailable(self_update::update::Release),
    Login,
    LoggingIn,
    Main,
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

pub fn default_platform() -> &'static str {
    if cfg!(windows) {
        return "windows";
    }
    if cfg!(unix) {
        return "linux";
    }
    if cfg!(target_os = "macos") {
        return "mac";
    }
    "unknown"
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
        self.downloading.subscription()
    }

    fn have_valid_config(&self) -> bool {
        self.blackboard.have_valid_config()
    }

    pub fn view(&self) -> Element<Message> {
        match &self.blackboard.screen {
            Screen::Wizard => self.wizard.view(&self.blackboard),
            Screen::Login => self.login.view(&self.blackboard),
            Screen::ClientUpdateAvailable(new_release) => {
                if self.is_updating_client {
                    return view_utils::container_with_title("Updating!".to_string(), column![]);
                }
                if !self.update_client_error.is_empty() {
                    return view_utils::container_with_title(
                        "Failed to update".to_string(),
                        column![button(text("Go to menu").center())],
                    );
                }
                let buttons_row = row![]
                    .push(
                        button(text("cancel").size(16).center())
                            .on_press(Message::GoToInitialScreen),
                    )
                    .push(
                        button(text("update").size(16).center())
                            .on_press(Message::UpdateClient(new_release.clone())),
                    )
                    .spacing(20);

                let content = column![]
                    .push(
                        text(format!(
                            "{} -> {}",
                            cargo_crate_version!(),
                            new_release.version
                        ))
                        .size(32),
                    )
                    .push(vertical_space().height(30))
                    .push(buttons_row);
                view_utils::container_with_title("New version available!".to_string(), content)
            }
            Screen::LoggingIn => Container::new(column![text("logging in")
                .size(40)
                .color(Color::parse("#417495").unwrap())])
            .center(0)
            .into(),
            Screen::Main => {
                match &self.run_from_args_issue {
                    RunFromArgsIssue::Error(_) | RunFromArgsIssue::FoundUpdate(..) => {
                        return Self::display_run_from_args_issue(&self.run_from_args_issue);
                    }
                    _ => {}
                }
                let games = self.blackboard.config.get_account_games();
                let game_count = games.len();
                let content = match &self.blackboard.selected_game {
                    None if game_count > 0 => {
                        column![text("Welcome!").size(48), "Select game to the left"]
                    }
                    None => column![
                        text("Welcome!").size(48),
                        "Found no games for your account, try refreshing",
                        vertical_space().height(20),
                        button("Refresh").on_press(Message::FetchGames)
                    ],
                    Some(game) => self.display_game_with_download_state(game),
                }
                .align_x(Center)
                .width(Fill);

                container_with_top_bar_and_side_view(content, self.blackboard.clone())
            }
        }
    }
    fn display_run_from_args_issue(issue: &RunFromArgsIssue) -> Element<Message> {
        match issue {
            RunFromArgsIssue::Error(message) => container(
                column![]
                    .push(text(message))
                    .push(button(text("close")).on_press(Message::ClearRequestedGameToPlay)),
            )
            .width(Fill)
            .height(Fill)
            .center(Fill)
            .into(),
            RunFromArgsIssue::FoundUpdate(game, new_release, installed_release) => container(
                column![].push(text("Found newer release, update?")).push(
                    row![]
                        .push(
                            button(text("update"))
                                .on_press(Message::Download(game.clone(), new_release.clone())),
                        )
                        .push(
                            button(text("play")).on_press(Message::Run(installed_release.clone())),
                        )
                        .spacing(10),
                ),
            )
            .width(Fill)
            .height(Fill)
            .center(Fill)
            .into(),
            _ => column![].into(),
        }
    }

    fn display_game_with_download_state(&self, game: &Game) -> Column<Message> {
        let download_state = match &self.blackboard.selected_game {
            None => DownloadState::Idle,
            Some(game) => {
                let state = self
                    .downloading
                    .downloads
                    .iter()
                    .find(|x| x.game_name_id == game.name_id);
                match state {
                    None => DownloadState::Idle,
                    Some(download) => download.state.clone(),
                }
            }
        };

        match download_state {
            DownloadState::Idle => self.show_game(game),
            DownloadState::Downloading {
                progress_percentage: progress,
            } => column![
                text("Downloading Release!").size(24),
                vertical_space().height(20),
                progress_bar(0.0..=100.0, progress.clone()).width(200)
            ]
            .align_x(Center)
            .width(Fill),
            DownloadState::Errored(reason) => {
                let reason_str = match reason {
                    DownloadError::RequestFailed(e) => format!("request error:  {}", e),
                    DownloadError::IoError => "io error".to_string(),
                };
                column![
                    text(format!(
                        "Failed to download release with error: {}",
                        reason_str
                    )),
                    button(text("Ok").center())
                        .on_press(Message::CloseDownloadError(game.name_id.to_string()))
                ]
            }
        }
    }

    fn newest_release_by_state(
        releases: &[Release],
        channel: Option<&str>,
        state: Option<ReleaseState>,
    ) -> Option<Release> {
        releases
            .iter()
            .filter(|x| channel.map_or(true, |c| x.channel_name == c))
            .filter(|x| state.as_ref().map_or(true, |s| &x.state == s))
            .max_by(|x, y| x.release_date.cmp(&y.release_date))
            .map(|x| x.clone())
    }

    fn show_game(&self, game: &Game) -> Column<Message> {
        if game.releases.is_empty() {
            return column![text("found no releases")];
        }
        let channel = match &self.blackboard.selected_channel {
            None => {
                return column![text("no channel set")];
            }
            Some(c) => c,
        };

        let newest_installed = Self::newest_release_by_state(
            &game.releases,
            Some(channel),
            Some(ReleaseState::Installed),
        );
        let latest_release = Self::newest_release_by_state(&game.releases, Some(channel), None);

        let option_button = match newest_installed {
            None => match latest_release {
                None => button(text("Fetch releases").center()).on_press(Message::FetchGames),
                Some(latest) => button(text("Install").center())
                    .on_press(Message::Download(game.clone(), latest.clone())),
            },
            Some(release) => {
                let play_button =
                    button(text("Play").center()).on_press(Message::Run(release.clone()));
                if let Some(latest) = latest_release {
                    if &latest.version != &release.version {
                        button(text("Update").center())
                            .on_press(Message::Download(game.clone(), latest.clone()))
                    } else {
                        play_button
                    }
                } else {
                    play_button
                }
            }
        }
        .width(75);

        let (versions, channels) =
            game.releases
                .iter()
                .fold((HashSet::new(), HashSet::new()), |(mut a, mut b), c| {
                    // Only show version if is selected channel
                    if self
                        .blackboard
                        .selected_channel
                        .as_ref()
                        .is_some_and(|x| x == &c.channel_name)
                    {
                        a.insert((c.version.to_string(), c.description.to_string()));
                    }
                    b.insert(c.channel_name.to_string());
                    (a, b)
                });

        let mut channels = channels
            .iter()
            .map(|y| y.to_string())
            .collect::<Vec<String>>();
        channels.sort();

        let dropdown_picker = self.blackboard.selected_channel.is_some().then_some(
            pick_list(
                channels,
                self.blackboard.selected_channel.as_ref(),
                Message::SelectedChannelChanged,
            )
            .width(100),
        );

        let installed_versions: Vec<String> = game
            .releases
            .iter()
            .filter(|x| &x.channel_name == channel)
            .filter(|x| x.state == ReleaseState::Installed)
            .map(|x| x.version.to_string())
            .collect();

        let installed_versions_picker = installed_versions.len().gt(&0).then_some(
            pick_list(
                installed_versions,
                self.blackboard.selected_version.as_ref(),
                Message::SelectedVersionChanged,
            )
            .width(100),
        );

        let buttons = row![]
            .push(option_button)
            .push_maybe(dropdown_picker)
            .push_maybe(installed_versions_picker)
            .padding(10)
            .spacing(20)
            .width(300);

        let mut versions: Vec<(String, String)> = versions.into_iter().map(|x| x).collect();
        versions.sort_by(|(_, x), (_, y)| y.cmp(x));

        let versions = versions
            .into_iter()
            .fold(column![], |c, (version, description)| {
                c.push(text(version).size(16))
                    .push(text(description).size(12))
            })
            .spacing(10);

        let c = Container::new(
            column![
                text(game.name.to_string())
                    .size(32)
                    .width(450)
                    .align_x(Center),
                vertical_space().height(3),
                column![buttons].align_x(Center),
                vertical_space().height(20),
                text("Description").size(20),
                vertical_space().height(2),
                text(game.description.to_string())
                    .size(14)
                    .width(450)
                    .align_x(Center),
                vertical_space().height(15),
                text("Releases").size(20),
                vertical_space().height(2),
            ]
            .align_x(Center)
            .width(Fill),
        )
        .width(Fill)
        .height(210);

        column![c, scrollable(versions).width(400)]

        //
        //    scrollable(versions).width(400)
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
                let release = Self::newest_release_by_state(
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
        let release = Self::newest_release_by_state(
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
        match Self::newest_release_by_state(&game.releases, Some(&channel), None) {
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
            Message::UpdateClient(release) => {
                self.is_updating_client = true;
                let result = Self::download_newer_version_and_replace(release);
                match result {
                    Ok(_) => {}
                    Err(e) => {
                        self.is_updating_client = false;
                        self.update_client_error = e.to_string();
                    }
                }
            }
            Message::ClearRequestedGameToPlay => self.requested_game_to_play = None,
            Message::GoToScreen(screen) => {
                self.blackboard.screen = screen;
            }
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

            Message::Logout => {
                self.logout();
            }
            Message::ConfigOpened(result) => {
                return self.handle_config_open(result);
            }
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
                    FetchGamesError::APIError(ref inner) => {
                        println!("api error: {}", &inner)
                    }
                    FetchGamesError::Unreachable(ref inner) => {
                        println!("api error: {}", &inner);
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

                self.run_from_args_issue = self.handle_args_game_running();
                if let RunFromArgsIssue::CanPlay(release) = &self.run_from_args_issue {
                    self.blackboard
                        .run_release(&self.requested_game_to_play.as_ref().unwrap(), &release);
                    self.run_from_args_issue = RunFromArgsIssue::NotSet;
                }
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
            println!("failed to open config, recreating {}", error_message);
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

        match Self::look_for_newer_version() {
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

    fn download_newer_version_and_replace(
        release: self_update::update::Release,
    ) -> Result<(), anyhow::Error> {
        // get the first available release
        let asset = release.asset_for(default_platform(), None).unwrap();

        println!("creating temp dirs");
        let cur_dir = env::current_dir().context("getting cur dir")?;
        let tmp_dir = tempfile::Builder::new()
            .prefix("self_update")
            .tempdir_in(cur_dir)
            .context("creating temp dir")?;
        let tmp_zip_path = tmp_dir.path().join(&asset.name);
        let tmp_zip = std::fs::File::create(&tmp_zip_path).context("opening zip file")?;

        println!("downloading");
        self_update::Download::from_url(&asset.download_url)
            .set_header(reqwest::header::ACCEPT, "application/octet-stream".parse()?)
            .download_to(&tmp_zip)?;

        let bin_name_suffix = match default_platform() {
            "windows" => ".exe",
            _ => "",
        };
        println!("updating!");
        let bin_name = std::path::PathBuf::from(format!("drops-client{}", bin_name_suffix));
        println!("using binname: {}", bin_name.to_str().unwrap_or(""));
        self_update::Extract::from_source(&tmp_zip_path)
            .archive(self_update::ArchiveKind::Zip)
            .extract_file(tmp_dir.path(), &bin_name)?;
        println!("replacing!");

        let new_exe = tmp_dir.path().join(bin_name);
        self_replace::self_replace(new_exe)?;

        Ok(())
    }
    fn look_for_newer_version() -> Result<Option<self_update::update::Release>, anyhow::Error> {
        let releases = self_update::backends::github::ReleaseList::configure()
            .repo_owner("kralle333")
            .repo_name("drops-client")
            .build()?
            .fetch()?;
        println!("found releases:");
        println!("{:#?}\n", releases);

        if releases.is_empty() {
            return Ok(None);
        }

        let newer = releases.into_iter().nth(0).unwrap();
        let newer_version = newer.version.to_string();

        let current = cargo_crate_version!();
        if version::bump_is_greater(current, &newer_version).map(|x| !x)? {
            println!("no updates");
            return Err(anyhow!("no update"));
        }

        Ok(Some(newer))
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

fn main() -> iced::Result {
    env_logger::Builder::from_env(Env::default().default_filter_or("info")).init();

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
}
