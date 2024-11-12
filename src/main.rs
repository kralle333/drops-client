#![windows_subsystem = "windows"]
mod api;
mod blackboard;
mod client_config;
mod downloading;
mod errors;
mod handlers;
mod messages;
mod tasks;

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
use blackboard::Blackboard;
use env_logger::Env;
use iced::widget::{
    button, column, container, horizontal_space, pick_list, progress_bar, row, scrollable, text,
    text_input, vertical_space, Column, Container, TextInput,
};
use iced::{window, Center, Color, Element, Fill, Size, Task};
use iced_futures::Subscription;
use log::error;
use secrecy::{ExposeSecret, SecretString};
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
}

#[derive(Default, Clone, Debug)]
pub enum Screen {
    #[default]
    Wizard,
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
        self.blackboard.config.is_active
    }

    fn login_column(&self) -> Column<Message> {
        let options = self
            .blackboard
            .config
            .accounts
            .iter()
            .map(|x| x.url.to_string())
            .collect::<Vec<String>>();

        let active_one = self.blackboard.config.get_active_account();
        let default = active_one.map(|account| {
            self.blackboard
                .config
                .accounts
                .iter()
                .find(|x| x.id == account.id)
                .unwrap()
                .url
                .to_string()
        });

        let server_select = pick_list(options, default, Message::ServerChanged).width(250);

        let username_input: TextInput<Message> = text_input("Username", &self.login.username_input)
            .on_input(Message::UsernameChanged)
            .padding(10)
            .size(15)
            .width(250);
        let password_input =
            iced::widget::text_input("Password", &self.login.password_input.expose_secret())
                .on_input(Message::PasswordChanged)
                .secure(true)
                .padding(10)
                .size(15)
                .width(250);

        let login_button = iced::widget::button(text("login").center())
            .on_press(Message::Login)
            .padding(10)
            .width(200);

        let new_server_button = button(text("new server").center())
            .on_press(Message::GoToScreen(Screen::Wizard))
            .padding(5)
            .width(150);

        let inputs = column![]
            .push_maybe(
                self.login
                    .error_reason
                    .clone()
                    .map(|x| text(format!("{}", x)).color(Color::from_rgb(0.8, 0.4, 0.4))),
            )
            .push(server_select)
            .push(username_input)
            .push(password_input)
            .spacing(10);

        column![]
            .push(row![horizontal_space(), inputs, horizontal_space()])
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
                .width(200)
                .on_input(Message::DropsUrlChanged)
                .padding(10)
                .size(15);

        let button_width = 80;
        let button_height = 40;
        let can_test =
            !self.wizard.is_checking_host_reachable && !self.wizard.drops_url_input.is_empty();
        let button_work = can_test.then_some(true);

        let host_err_text = match self.wizard.has_valid_host {
            true => text("ok").color(Color::from_rgb(0.4, 0.7, 0.4)),
            false => text(self.wizard.host_error.to_string()).color(Color::from_rgb(0.8, 0.4, 0.4)),
        };
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

        let dir_select_input = text_input("select games dir", &self.wizard.games_dir_input)
            .width(200)
            .padding(10)
            .size(15);

        let ok_text = match self.wizard.has_valid_games_dir {
            true => "ok",
            false => "",
        };
        let select_file_row = row![]
            .push(dir_select_input)
            .push(
                button(text("open").center())
                    .on_press(Message::SelectGamesDir)
                    .width(button_width)
                    .height(button_height),
            )
            .spacing(20)
            .align_y(Center);

        let should_show = match !(!self.wizard.has_valid_games_dir || !self.wizard.has_valid_host) {
            true => Some(true),
            false => None,
        };
        let cancel_button = match self.have_valid_config() {
            true => Some(
                button("cancel")
                    .on_press(Message::GoToScreen(Screen::Login))
                    .padding(10),
            ),
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
            host_err_text,
            test_host_row,
            vertical_space().height(10),
            text(ok_text).color(Color::from_rgb(0.4, 0.7, 0.4)),
            select_file_row,
            vertical_space().height(80),
            bottom_bar
        ]
        .width(500)
        .spacing(0)
    }

    pub fn view(&self) -> Element<Message> {
        match self.blackboard.screen {
            Screen::Wizard | Screen::Login => {
                let (title, col) = match self.blackboard.screen {
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

                Container::new(r).center(0).width(Fill).height(Fill).into()
            }
            Screen::LoggingIn => Container::new(column![text("logging in")
                .size(40)
                .color(Color::parse("#417495").unwrap())])
            .center(0)
            .into(),
            Screen::Main => {
                match &self.run_from_args_issue {
                    RunFromArgsIssue::Error(message) => {
                        return container(column![].push(text(message)).push(
                            button(text("close")).on_press(Message::ClearRequestedGameToPlay),
                        ))
                        .width(Fill)
                        .height(Fill)
                        .center(Fill)
                        .into();
                    }
                    RunFromArgsIssue::FoundUpdate(game, new_release, installed_release) => {
                        return container(
                            column![].push(text("Found newer release, update?")).push(
                                row![]
                                    .push(button(text("update")).on_press(Message::Download(
                                        game.clone(),
                                        new_release.clone(),
                                    )))
                                    .push(
                                        button(text("play"))
                                            .on_press(Message::Run(installed_release.clone())),
                                    )
                                    .spacing(10),
                            ),
                        )
                        .width(Fill)
                        .height(Fill)
                        .center(Fill)
                        .into();
                    }
                    _ => {}
                }

                let header = container(
                    row![
                        text(format!(
                            "Logged in as  {}",
                            self.blackboard.config.get_username()
                        )),
                        horizontal_space(),
                        "drops",
                        horizontal_space(),
                        button(text("logout").center()).on_press(Message::Logout)
                    ]
                    .padding(10)
                    .align_y(Center),
                );

                let games = self.blackboard.config.get_account_games();
                let game_count = games.len();
                let games: Element<Message> = column(games.into_iter().map(|x| {
                    button(text(x.name.to_string()).center())
                        .width(Fill)
                        .on_press(Message::SelectGame(x.clone()))
                        .into()
                }))
                .spacing(10)
                .into();

                let sidebar_column = column![
                    row![
                        horizontal_space(),
                        text("Games").align_x(Center).size(22),
                        horizontal_space()
                    ],
                    vertical_space().height(15),
                    games,
                    vertical_space()
                ]
                .padding(10)
                .width(160);

                let sidebar = container(sidebar_column)
                    .style(container::dark)
                    .center_y(Fill);

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

                let content = container(scrollable(match &self.blackboard.selected_game {
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
                                button(text("Ok").center()).on_press(Message::CloseDownloadError(
                                    game.name_id.to_string()
                                ))
                            ]
                            .spacing(40)
                            .align_x(Center)
                            .width(Fill)
                            .into()
                        }
                    },
                }))
                .height(Fill)
                .padding(10);

                column![header, row![sidebar, content]].into()
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
        };

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

        let dropdown = pick_list(
            channels,
            self.blackboard.selected_channel.as_ref(),
            Message::ChannelChanged,
        );

        let dropdown = match self.blackboard.selected_channel {
            None => None,
            Some(_) => Some(dropdown),
        };

        let buttons = row![]
            .push(option_button)
            .push_maybe(dropdown)
            .padding(10)
            .spacing(10)
            .width(200);

        let mut versions: Vec<(String, String)> = versions.into_iter().map(|x| x).collect();
        versions.sort_by(|(_, x), (_, y)| y.cmp(x));

        let versions = versions
            .into_iter()
            .fold(column![], |c, (version, description)| {
                c.push(text(version).size(16))
                    .push(text(description).size(12))
            })
            .spacing(10);

        column![
            text(game.name.to_string())
                .size(32)
                .width(450)
                .align_x(Center),
            vertical_space().height(3),
            row![buttons],
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
            scrollable(versions).width(400)
        ]
        .align_x(Center)
        .width(Fill)
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
                return tasks::perform_fetch_games_from_config(&self.blackboard.config);
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

            Message::ChannelChanged(channel_name) => {
                self.blackboard.selected_channel = Some(channel_name);
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

        let mut fetch_games = false;
        self.blackboard.screen = match self.have_valid_config() {
            true if self.blackboard.config.has_session_token() => {
                fetch_games = true;
                Screen::Main
            }
            true => Screen::Login,
            false => Screen::Wizard,
        };

        if fetch_games {
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
