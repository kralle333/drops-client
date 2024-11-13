use crate::blackboard::Blackboard;
use crate::handlers::MessageHandler;
use crate::messages::Message;
use crate::Screen::LoggingIn;
use crate::{tasks, view_utils, Screen};
use iced::widget::{
    button, column, horizontal_space, pick_list, row, text, text_input, vertical_space, Column,
    TextInput,
};
use iced::{Color, Element, Task};
use log::error;
use secrecy::{ExposeSecret, SecretString};

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
    pub fn view(&self, blackboard: &Blackboard) -> Element<Message> {
        view_utils::container_with_title("drops".to_string(), self.login_column(blackboard)).into()
    }
    fn login_column(&self, blackboard: &Blackboard) -> Column<Message> {
        let options = blackboard
            .config
            .accounts
            .iter()
            .map(|x| x.url.to_string())
            .collect::<Vec<String>>();

        let active_one = blackboard.config.get_active_account();
        let default = active_one.map(|account| {
            blackboard
                .config
                .accounts
                .iter()
                .find(|x| x.id == account.id)
                .unwrap()
                .url
                .to_string()
        });

        let server_select = pick_list(options, default, Message::ServerChanged).width(250);

        let username_input: TextInput<Message> = text_input("Username", &self.username_input)
            .on_input(Message::UsernameChanged)
            .padding(10)
            .size(15)
            .width(250);
        let password_input =
            iced::widget::text_input("Password", &self.password_input.expose_secret())
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
                self.error_reason
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
