use crate::api::can_reach_host;
use crate::blackboard::Blackboard;
use crate::client_config::DropsAccountConfig;
use crate::messages::Message;
use crate::{view_utils, Screen};
use iced::widget::{
    button, column, horizontal_space, row, text, text_input, vertical_space, Column,
};
use iced::{Center, Color, Element, Task};
use log::error;
use rfd::FileDialog;
use uuid::Uuid;

#[derive(Default)]
pub struct WizardMessageHandler {
    pub(crate) has_valid_host: bool,
    pub(crate) games_dir_input: String,
    pub(crate) has_valid_games_dir: bool,
    pub(crate) drops_url_input: String,
    pub(crate) is_checking_host_reachable: bool,
    pub(crate) host_error: String,
}

impl WizardMessageHandler {
    pub(crate) fn clear_input(&mut self) {
        self.games_dir_input = "".to_string();
        self.drops_url_input = "".to_string();
    }
    pub fn view(&self, blackboard: &Blackboard) -> Element<Message> {
        view_utils::container_with_title("Welcome".to_string(), self.wizard_column(blackboard))
    }
    fn wizard_column(&self, blackboard: &Blackboard) -> Column<Message> {
        let host_input = text_input("drops server url", &self.drops_url_input)
            .width(200)
            .on_input(Message::DropsUrlChanged)
            .padding(10)
            .size(15);

        let button_width = 80;
        let button_height = 40;
        let can_test = !self.is_checking_host_reachable && !self.drops_url_input.is_empty();
        let button_work = can_test.then_some(true);

        let host_err_text = match self.has_valid_host {
            true => text("ok").color(Color::from_rgb(0.4, 0.7, 0.4)),
            false => text(self.host_error.to_string()).color(Color::from_rgb(0.8, 0.4, 0.4)),
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

        let dir_select_input = text_input("select games dir", &self.games_dir_input)
            .width(200)
            .padding(10)
            .size(15);

        let ok_text = match self.has_valid_games_dir {
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

        let should_show = match !(!self.has_valid_games_dir || !self.has_valid_host) {
            true => Some(true),
            false => None,
        };
        let cancel_button = match blackboard.have_valid_config() {
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
}

impl WizardMessageHandler {
    fn check_host_reachable(&mut self, url: &str) -> Task<Message> {
        self.is_checking_host_reachable = true;
        Task::perform(
            can_reach_host(url.to_string()),
            Message::WizardCanReachHostChecked,
        )
    }
    pub fn update(&mut self, message: Message, blackboard: &mut Blackboard) -> Task<Message> {
        match message {
            Message::SelectGamesDir => {
                let file = FileDialog::new().pick_folder();
                if let Some(dir) = file {
                    if let Some(dir_string) = dir.to_str() {
                        self.games_dir_input = dir_string.to_string();
                        self.has_valid_games_dir = true;
                    }
                }
            }
            Message::DropsUrlChanged(s) => {
                self.drops_url_input = s;
                self.has_valid_host = false;
            }
            Message::FinishWizard => {
                let account = DropsAccountConfig {
                    id: Uuid::new_v4(),
                    url: self.drops_url_input.to_string(),
                    games_dir: self.games_dir_input.to_string(),
                    username: "".to_string(),
                    session_token: Default::default(),
                    games: vec![],
                };
                blackboard.config.is_active = true;
                blackboard.config.active_account = account.id;
                blackboard.config.accounts.push(account.clone());
                blackboard.config.save().expect("Failed to store config!");
                blackboard.screen = Screen::Login;
            }
            Message::TestDropsUrl => {
                return self.check_host_reachable(&self.drops_url_input.to_string())
            }
            Message::WizardCanReachHostChecked(Err(reason)) => {
                self.host_error = reason;
                self.is_checking_host_reachable = false;
            }
            Message::WizardCanReachHostChecked(Ok(())) => {
                self.has_valid_host = true;
                self.host_error = String::new();
                self.is_checking_host_reachable = false;
            }

            _ => error!("invalid wizard state!: {:?}", message),
        }
        Task::none()
    }
}
