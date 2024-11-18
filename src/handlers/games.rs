use crate::blackboard::Blackboard;
use crate::client_config::ReleaseState;
use crate::handlers::MessageHandler;
use crate::messages::Message;
use crate::{utils, view_utils};
use iced::widget::{button, column, text, vertical_space, Button};
use iced::widget::{pick_list, row, scrollable, Container};
use iced::{Center, Element, Fill, Task};
use log::error;
use std::collections::HashSet;

#[derive(Default)]
pub struct GamesMessageHandler;

impl GamesMessageHandler {
    pub fn view<'a>(&self, blackboard: &'a Blackboard) -> Element<'a, Message> {
        let games = blackboard.config.get_account_games();
        let game_count = games.len();
        let content = match &blackboard.selected_game {
            None if game_count > 0 => Container::new(
                column![text("Welcome!").size(48), "Select game to the left"].align_x(Center),
            )
            .width(Fill),
            None => Container::new(
                column![
                    text("Welcome!").size(48),
                    "Found no games for your account, try refreshing",
                    vertical_space().height(20),
                    button("Refresh").on_press(Message::FetchGames)
                ]
                .align_x(Center),
            )
            .width(Fill),
            Some(game) => {
                if game.releases.is_empty() {
                    return Container::new(column![text("found no releases")])
                        .width(Fill)
                        .into();
                }
                let channel = match &blackboard.selected_channel {
                    None => {
                        return column![text("no channel set")].into();
                    }
                    Some(c) => c,
                };

                let newest_installed = utils::newest_release_by_state(
                    &game.releases,
                    Some(channel),
                    Some(ReleaseState::Installed),
                );
                let latest_release =
                    utils::newest_release_by_state(&game.releases, Some(channel), None);

                let option_button = match newest_installed {
                    None => match latest_release {
                        None => {
                            button(text("Fetch releases").center()).on_press(Message::FetchGames)
                        }
                        Some(latest) => button(text("Install").center())
                            .on_press(Message::Download(game.clone(), latest.clone())),
                    },
                    Some(release) => {
                        let play_button: Button<Message> =
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

                let (versions, channels) = game.releases.iter().fold(
                    (HashSet::new(), HashSet::new()),
                    |(mut a, mut b), c| {
                        // Only show version if is selected channel
                        if blackboard
                            .selected_channel
                            .as_ref()
                            .is_some_and(|x| x == &c.channel_name)
                        {
                            a.insert((c.version.to_string(), c.description.to_string()));
                        }
                        b.insert(c.channel_name.to_string());
                        (a, b)
                    },
                );

                let mut channels = channels
                    .iter()
                    .map(|y| y.to_string())
                    .collect::<Vec<String>>();
                channels.sort();

                let dropdown_picker = blackboard.selected_channel.is_some().then_some(
                    pick_list(
                        channels,
                        blackboard.selected_channel.as_ref(),
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
                        blackboard.selected_version.as_ref(),
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

                Container::new(column![c, scrollable(versions).width(400)])
            }
        };
        view_utils::container_with_top_bar_and_side_view(content, blackboard).into()
    }
}

impl MessageHandler for GamesMessageHandler {
    fn update(&mut self, message: Message, blackboard: &mut Blackboard) -> Task<Message> {
        match message {
            Message::SelectGame(game) => {
                blackboard.selected_channel = match game.selected_channel.as_ref() {
                    None => game.releases.first().map(|x| x.channel_name.to_string()),
                    Some(channel) => Some(channel.to_string()),
                };
                let selected_channel = blackboard.selected_channel.as_ref().unwrap().to_string();
                let mut versions_installed: Vec<String> = game
                    .releases
                    .iter()
                    .filter(|x| &x.channel_name == &selected_channel)
                    .filter(|x| x.state == ReleaseState::Installed)
                    .map(|x| x.version.to_string())
                    .collect();
                versions_installed.sort_by(|x, y| y.cmp(&x));
                blackboard.selected_version = versions_installed.first().map(|x| x.to_string());

                blackboard.selected_game = Some(game);
            }

            Message::Run(release) => {
                let game = blackboard.selected_game.as_ref().unwrap();
                blackboard.run_release(&game.clone(), &release)
            }
            _ => {
                error!("Unexpected state!")
            }
        }
        Task::none()
    }
}
