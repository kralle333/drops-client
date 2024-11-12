use crate::handlers::MessageHandler;
use crate::messages::Message;
use crate::Screen::LoggingIn;
use crate::{tasks, Screen};
use iced::Task;
use log::error;
use secrecy::{ExposeSecret, SecretString};
use crate::blackboard::Blackboard;

#[derive(Default)]
pub struct LoginMessageHandler {
    pub(crate) username_input: String,
    pub(crate) password_input: SecretString,
    pub(crate) error_reason: Option<String>,
}

impl LoginMessageHandler {
    pub(crate) fn set_username(&mut self, username: &str) {
        self.username_input = username.to_string();
    }
}

impl MessageHandler for LoginMessageHandler {
    fn update(&mut self, message: Message, blackboard: &mut Blackboard) -> Task<Message> {
        match message {
            Message::Login => {
                blackboard.screen = LoggingIn;
                let url = blackboard.config.get_drops_url();
                return tasks::perform_login(
                    &url,
                    &self.username_input,
                    &self.password_input.expose_secret(),
                );
            }

            Message::LoggedInFinished(result) => match result {
                Ok(token) => {
                    blackboard
                        .config
                        .set_username_and_save(&self.username_input);
                    blackboard.config.set_session_token(token);
                    blackboard.config.save().expect("failed to save config");
                    blackboard.screen = Screen::Main;
                    return tasks::perform_fetch_games_from_config(&blackboard.config);
                }
                Err(e) => {
                    self.error_reason = Some(format!("{:?}", e));
                    blackboard.screen = Screen::Login;
                }
            },
            Message::UsernameChanged(s) => self.username_input = s,
            Message::PasswordChanged(s) => self.password_input = SecretString::new(s.into()),
            Message::ServerChanged(s) => {
                self.username_input.clear();
                blackboard.config.set_active_account_by_url(s);
            }
            _ => {
                error!("invalid login state message: {:?}", message)
            }
        }
        Task::none()
    }
}
