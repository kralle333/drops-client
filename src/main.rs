mod api;
mod client_config;
mod downloading;
mod errors;
mod messages;

use crate::api::{can_reach_host, fetch_games, login};
use crate::client_config::ReleaseState::Installed;
use crate::client_config::{ClientConfig, DropsAccountConfig, Game};
use crate::downloading::{Download, DownloadProgress, DownloadState};
use crate::errors::{FetchGamesError, LoginError};
use crate::messages::Message;
use env_logger::Env;
use iced::widget::{
    button, column, container, horizontal_space, pick_list, progress_bar, row, scrollable, text,
    text_input, vertical_space, Column, Container, TextInput,
};
use iced::{window, Center, Color, Element, Fill, Size, Task};
use iced_futures::Subscription;
use log::error;
use rfd::FileDialog;
use secrecy::{ExposeSecret, SecretString};
use std::collections::HashSet;
use std::default::Default;
use std::future::Future;
use std::path::PathBuf;
use std::process::Command;
use uuid::Uuid;

#[derive(Default)]
struct DropsClient {
    login: LoginInput,
    wizard: WizardInput,
    config: Option<ClientConfig>,
    is_checking_host_reachable: bool,
    screen: Screen,
    selected_game: Option<Game>,
    is_playing: bool,
    selected_channel: Option<String>,
    downloads: Vec<Download>,
}

#[derive(Default)]
struct LoginInput {
    username_input: String,
    password_input: SecretString,
    error_reason: Option<String>,
}

#[derive(Default)]
struct WizardInput {
    has_valid_games_dir: bool,
    has_valid_host: bool,
    drops_url_input: String,
    games_dir_input: String,
}

impl WizardInput {
    pub(crate) fn clear_input(&mut self) {
        self.drops_url_input = String::new();
        self.games_dir_input = String::new();
    }
}

#[derive(Default)]
enum Screen {
    #[default]
    Wizard,
    Login,
    LoggingIn,
    Main,
}

#[derive(Debug, Clone)]
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
        (
            DropsClient { ..Self::default() },
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
        Subscription::batch(self.downloads.iter().map(Download::subscription))
    }

    fn perform_login(&self) -> Task<Message> {
        Task::perform(
            login(
                self.config.clone().unwrap(),
                self.login.username_input.to_string(),
                self.login.password_input.expose_secret().to_string(),
            ),
            Message::LoggedInFinished,
        )
    }

    fn perform_fetch_games(&mut self) -> Task<Message> {
        let config = self.config.clone().unwrap();
        Task::perform(fetch_games(config), Message::GamesFetched)
    }

    fn check_host_reachable(&mut self, url: &str) -> Task<Message> {
        self.is_checking_host_reachable = true;
        Task::perform(
            can_reach_host(url.to_string()),
            Message::WizardCanReachHostChecked,
        )
    }

    fn login_column(&self) -> Column<Message> {
        let config = self.config.as_ref().unwrap();
        let options = config
            .accounts
            .iter()
            .map(|x| x.url.to_string())
            .collect::<Vec<String>>();

        let active_one = config.get_active_account();
        let default = match active_one {
            None => None,
            Some(account) => Some(
                config
                    .accounts
                    .iter()
                    .find(|x| x.id == account.id)
                    .unwrap()
                    .url
                    .to_string(),
            ),
        };

        let server_select = pick_list(options, default, Message::ServerChanged);

        let username_input: TextInput<Message> = text_input("Username", &self.login.username_input)
            .on_input(Message::UsernameChanged)
            .padding(10)
            .size(15);
        let password_input =
            iced::widget::text_input("Password", &self.login.password_input.expose_secret())
                .on_input(Message::PasswordChanged)
                .secure(true)
                .padding(10)
                .size(15);

        let login_button = iced::widget::button(text("login").center())
            .on_press(Message::Login)
            .padding(10)
            .width(200);

        let new_server_button = button(text("new server").center())
            .on_press(Message::GoToWizard)
            .padding(5)
            .width(150);

        column![]
            .push_maybe(
                self.login
                    .error_reason
                    .clone()
                    .map(|x| text(format!("{}", x)).color(Color::from_rgb(0.8, 0.4, 0.4))),
            )
            .push(server_select)
            .push(username_input)
            .push(password_input)
            .spacing(10)
            .push(vertical_space().height(5))
            .push(row![horizontal_space(), login_button, horizontal_space()])
            .push(vertical_space().height(10))
            .push(row![
                horizontal_space(),
                new_server_button,
                horizontal_space()
            ])
            .push(vertical_space().height(50))
    }

    fn wizard_column(&self) -> Column<Message> {
        let host_input: TextInput<Message> =
            text_input("drops server url", &self.wizard.drops_url_input)
                .on_input(Message::DropsUrlChanged)
                .padding(10)
                .size(15);

        let button_width = 80;
        let button_height = 40;
        let can_test = !self.is_checking_host_reachable && !self.wizard.drops_url_input.is_empty();
        let button_work = can_test.then_some(true);
        let test_host_row = row![]
            .push(host_input)
            .push(
                button(text("test").center())
                    .on_press_maybe(button_work.map(|_| Message::TestDropsUrl))
                    .width(button_width)
                    .height(button_height),
            )
            .spacing(20)
            .align_y(Center);

        let file_row = text_input("select games dir", &self.wizard.games_dir_input)
            .padding(10)
            .size(15);

        let select_file_row = row![]
            .push(file_row)
            .push(
                button(text("open").center())
                    .on_press(Message::SelectGamesDir)
                    .width(button_width)
                    .height(button_height),
            )
            .spacing(20)
            .align_y(Center);

        let should_show = match self.wizard.has_valid_games_dir && self.wizard.has_valid_host {
            true => Some(true),
            false => None,
        };
        let cancel_button = match self.config.as_ref().is_some() {
            true => Some(button("cancel").on_press(Message::GoToLogin).padding(10)),
            false => None,
        };

        let bottom_bar = row![]
            .push(horizontal_space())
            .push_maybe(cancel_button)
            .push(
                button("continue")
                    .on_press_maybe(should_show.map(|_| Message::FinishWizard))
                    .padding(10),
            )
            .push(horizontal_space())
            .spacing(30)
            .align_y(Center);
        column![
            test_host_row,
            select_file_row,
            vertical_space().height(10),
            bottom_bar
        ]
        .spacing(20)
    }

    pub fn view(&self) -> Element<Message> {
        match self.screen {
            Screen::Wizard | Screen::Login => {
                let (title, col) = match self.screen {
                    Screen::Login => ("drops", self.login_column()),
                    Screen::Wizard => ("welcome", self.wizard_column()),
                    _ => ("invalid", column![]),
                };

                // Centered box with title
                let col = column![]
                    .push(text(title).size(70))
                    .width(300)
                    .align_x(Center)
                    .spacing(70)
                    .push(col);

                let r = row![]
                    .push(horizontal_space())
                    .push(col)
                    .push(horizontal_space());

                Container::new(r)
                    .center(0)
                    .width(iced::Length::Fill)
                    .height(iced::Length::Fill)
                    .into()
            }
            Screen::LoggingIn => Container::new(column![text("logging in")
                .size(40)
                .color(Color::parse("#417495").unwrap())])
            .center(0)
            .into(),
            Screen::Main => {
                let header = container(
                    row![
                        text(format!(
                            "Logged in as  {}",
                            self.config.as_ref().unwrap().get_username()
                        )),
                        horizontal_space(),
                        "💧💧💧",
                        horizontal_space(),
                        button(text("logout").center()).on_press(Message::Logout)
                    ]
                    .padding(10)
                    .align_y(Center),
                );

                let games = self.config.as_ref().unwrap().get_account_games();
                let game_count = games.len();
                let games: Element<Message> = column(games.into_iter().map(|x| {
                    button(text(x.name.to_string()).center())
                        .width(Fill)
                        .on_press(Message::SelectGame(x.clone()))
                        .into()
                }))
                .into();

                let sidebar_column =
                    column![text("Games").align_x(Center), games, vertical_space()]
                        .spacing(40)
                        .padding(10)
                        .width(160);

                let sidebar = container(sidebar_column)
                    .style(container::dark)
                    .center_y(Fill);

                let download_state = match &self.selected_game {
                    None => DownloadState::Idle,
                    Some(game) => {
                        let state = self
                            .downloads
                            .iter()
                            .find(|x| x.game_name_id == game.name_id);
                        match state {
                            None => DownloadState::Idle,
                            Some(download) => download.state.clone(),
                        }
                    }
                };

                let content = container(scrollable(match &self.selected_game {
                    None if game_count > 0 => {
                        column![text("Welcome!").size(48), "Select game to the left"]
                            .spacing(40)
                            .align_x(Center)
                            .width(Fill)
                    }
                    None => column![
                        text("Welcome!").size(48),
                        "Found no games for your account, try refreshing",
                        vertical_space().height(20),
                        button("Refresh").on_press(Message::FetchGames)
                    ]
                    .spacing(40)
                    .align_x(Center)
                    .width(Fill),
                    Some(game) => match download_state {
                        DownloadState::Idle => self.show_game(game),
                        DownloadState::Downloading {
                            progress_percentage: progress,
                        } => column![
                            text("Downloading Release!").size(24),
                            vertical_space().height(20),
                            progress_bar(0.0..=100.0, progress).width(200)
                        ]
                        .align_x(Center)
                        .width(Fill),
                        DownloadState::Errored(reason) => column![
                            text(format!(
                                "Failed to download release with error: {:?}",
                                reason
                            )),
                            button(text("Ok").center())
                                .on_press(Message::CloseDownloadError(game.name_id.to_string()))
                        ]
                        .spacing(40)
                        .align_x(Center)
                        .width(Fill)
                        .into(),
                    },
                }))
                .height(Fill)
                .padding(10);

                column![header, row![sidebar, content]].into()
            }
        }
    }

    fn show_game(&self, game: &Game) -> Column<Message> {
        let newest_installed = game
            .releases
            .iter()
            .filter(|x| x.state == Installed)
            .max_by(|x, y| x.release_date.cmp(&y.release_date));
        let latest_release = game
            .releases
            .iter()
            .max_by(|x, y| x.release_date.cmp(&y.release_date));

        let option_button = match newest_installed {
            None => match latest_release {
                None => button(text("Fetch releases").center()).on_press(Message::FetchGames),
                Some(latest) => button(text("Install").center())
                    .on_press(Message::Install(game.clone(), latest.clone())),
            },
            Some(release) => {
                let play_button =
                    button(text("Play").center()).on_press(Message::Run(release.clone()));
                if let Some(latest) = latest_release {
                    if &latest.version != &release.version {
                        button(text("Update").center())
                            .on_press(Message::Install(game.clone(), latest.clone()))
                    } else {
                        play_button
                    }
                } else {
                    play_button
                }
            }
        };

        let (versions, channels) =
            game.releases
                .iter()
                .fold((HashSet::new(), HashSet::new()), |(mut a, mut b), c| {
                    a.insert(c.version.to_string());
                    b.insert(c.channel_name.to_string());
                    (a, b)
                });
        let channels: Vec<String> = channels.iter().map(|y| y.to_string()).collect();
        let dropdown = pick_list(
            channels,
            self.selected_channel.as_ref(),
            Message::ChannelChanged,
        );

        let dropdown = match self.selected_channel {
            None => None,
            Some(_) => Some(dropdown),
        };

        let buttons = row![]
            .push(option_button)
            .push_maybe(dropdown)
            .padding(10)
            .spacing(10)
            .width(200);

        let versions = versions
            .into_iter()
            .fold(column![], |c, version| c.push(text(version).size(16)))
            .spacing(10);
        column![
            text(game.name.to_string()).size(32),
            vertical_space().height(3),
            row![buttons],
            vertical_space().height(20),
            text("Description").size(20),
            vertical_space().height(2),
            text(game.description.to_string()).size(16),
            vertical_space().height(15),
            text("Releases").size(20),
            vertical_space().height(2),
            versions
        ]
        .align_x(Center)
        .width(Fill)
    }

    pub fn update(&mut self, message: Message) -> Task<Message> {
        match message {
            Message::GoToLogin => {
                self.screen = Screen::Login;
            }
            Message::GoToWizard => {
                self.screen = Screen::Wizard;
            }
            Message::DownloadProgressing((id, Ok(progress))) => match progress {
                DownloadProgress::Downloading { percent } => {
                    self.downloads
                        .iter_mut()
                        .find(|x| x.game_name_id == id)
                        .unwrap()
                        .state = DownloadState::Downloading {
                        progress_percentage: percent,
                    }
                }
                DownloadProgress::Finished { release } => {
                    let config = self.config.as_mut().unwrap();
                    if let Err(_) = config.update_install_state(
                        &release.game_name_id,
                        &release.version,
                        &release.channel_name,
                        Installed,
                    ) {
                        //TODO: Display error
                        error!("failed to update config install state");
                    }
                    config.save().expect("failed to save config!");
                    self.update_selected_game();

                    self.downloads.retain(|x| x.game_name_id != id);
                }
            },
            Message::DownloadProgressing((id, Err(error))) => {
                self.downloads
                    .iter_mut()
                    .find(|x| x.game_name_id == id)
                    .unwrap()
                    .state = DownloadState::Errored(error)
            }
            Message::CloseDownloadError(id) => {
                self.downloads.retain(|x| x.game_name_id != id);
            }
            Message::ConfigOpened(result) => {
                self.config = match result {
                    Ok(config) => Some(config),
                    Err(_) => None,
                };
                if self.config.is_some() {
                    self.login.username_input = self.config.as_ref().unwrap().get_username();
                }

                let mut fetch_games = false;
                self.screen = match self.config.as_ref() {
                    Some(config) if config.has_session_token() => {
                        fetch_games = true;
                        Screen::Main
                    }
                    Some(_) => Screen::Login,
                    None => Screen::Wizard,
                };
                if fetch_games {
                    return self.perform_fetch_games();
                }
            }
            Message::TestDropsUrl => {
                return self.check_host_reachable(&self.wizard.drops_url_input.to_string())
            }
            Message::Login => {
                self.screen = Screen::LoggingIn;
                return self.perform_login();
            }
            Message::Logout => {
                self.logout();
            }
            Message::LoggedInFinished(result) => match result {
                Ok(token) => {
                    let config = self.config.as_mut().unwrap();
                    config.set_username_and_save(&self.login.username_input);
                    config.set_session_token(token);
                    config.save().expect("failed to save config");

                    self.wizard.clear_input();
                    self.screen = Screen::Main;
                    return self.perform_fetch_games();
                }
                Err(e) => {
                    self.login.error_reason = Some(format!("{:?}", e));
                    self.screen = Screen::Login;
                }
            },
            Message::FetchGames => {
                return self.perform_fetch_games();
            }
            Message::SelectGamesDir => {
                let file = FileDialog::new().pick_folder();
                if let Some(dir) = file {
                    if let Some(dir_string) = dir.to_str() {
                        self.wizard.games_dir_input = dir_string.to_string();
                        self.wizard.has_valid_games_dir = true;
                    }
                }
            }
            Message::ServerChanged(s) => {
                self.login.username_input.clear();
                self.config.as_mut().unwrap().set_active_account_by_url(s);
            }
            Message::DropsUrlChanged(s) => {
                self.wizard.drops_url_input = s;
                self.wizard.has_valid_host = false;
            }
            Message::SelectGame(game) => {
                self.selected_channel = match game.selected_channel.as_ref() {
                    None => match game.releases.first() {
                        None => None,
                        Some(release) => Some(release.channel_name.to_string()),
                    },
                    Some(channel) => Some(channel.to_string()),
                };
                self.selected_game = Some(game);
            }
            Message::Run(release) => {
                let config = self.config.as_ref().unwrap();
                let executable_dir = PathBuf::new()
                    .join(&config.get_games_dir())
                    .join(&self.selected_game.as_ref().unwrap().name_id)
                    .join(&release.channel_name)
                    .join(&release.version);

                let executable_path = executable_dir.join(&release.executable_path);
                let mut child = Command::new(&executable_path)
                    .current_dir(&executable_dir)
                    .envs(std::env::vars())
                    .spawn()
                    .expect(&format!(
                        "Failed to run the binary at: {:?}",
                        executable_path
                    ));

                let output = child.wait();
                self.is_playing = true;
            }
            Message::Install(game, release) => {
                let config = self.config.as_ref().unwrap();
                self.downloads.push(Download::new(&release, &game, config));
            }
            Message::UsernameChanged(s) => self.login.username_input = s,
            Message::GamesFetched(Err(e)) => {
                match e {
                    FetchGamesError::APIError(_) => {}
                    FetchGamesError::Unreachable(_) => {}
                    FetchGamesError::NotFound => {}
                    FetchGamesError::BadCredentials => {}
                    FetchGamesError::NeedRelogin => {
                        self.screen = Screen::Login;
                        self.config.as_mut().unwrap().clear_session_token();
                    }
                }
                error!("failed to fetch games! {:?}", e)
            }
            Message::GamesFetched(Ok(games_response)) => {
                self.config
                    .as_mut()
                    .unwrap()
                    .sync_and_save(games_response)
                    .expect("Failed to receive games response");
            }
            Message::PasswordChanged(s) => self.login.password_input = SecretString::new(s.into()),
            Message::WizardCanReachHostChecked(can_reach) => {
                self.wizard.has_valid_host = can_reach;
                self.is_checking_host_reachable = false;
            }
            Message::FinishWizard => {
                let account = DropsAccountConfig {
                    id: Uuid::new_v4(),
                    url: self.wizard.drops_url_input.to_string(),
                    games_dir: self.wizard.games_dir_input.to_string(),
                    username: "".to_string(),
                    session_token: "".to_string(),
                    games: vec![],
                };
                let mut config = self.config.clone().unwrap_or(ClientConfig::default());
                config.active_account = account.id;
                config.accounts.push(account.clone());
                config.save().expect("Failed to store config!");
                self.config = Some(config);
                self.screen = Screen::Login;
            }
            Message::ChannelChanged(channel_name) => {
                self.selected_channel = Some(channel_name);
            }
        }
        Task::none()
    }

    fn update_selected_game(&mut self) {
        let config = self.config.as_mut().unwrap();
        if self.selected_game.is_none() {
            return;
        }
        let game = self.selected_game.as_ref().unwrap();
        let updated_game = config
            .get_account_games()
            .iter()
            .find(|x| x.name_id == game.name_id)
            .unwrap()
            .clone();
        self.selected_game = Some(updated_game);
    }

    fn logout(&mut self) {
        self.selected_game = None;
        self.selected_channel = None;
        self.wizard.clear_input();
        self.is_playing = false;

        self.login.password_input = SecretString::new("".into());
        self.login.username_input.clear();
        self.login.error_reason = None;

        self.screen = Screen::Login;
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
