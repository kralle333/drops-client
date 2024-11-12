use crate::api::can_reach_host;
use crate::client_config::DropsAccountConfig;
use crate::messages::Message;
use crate::{Screen, SessionToken};
use iced::Task;
use log::error;
use rfd::FileDialog;
use uuid::Uuid;
use crate::blackboard::Blackboard;

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
                    session_token: SessionToken("".to_string()),
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
