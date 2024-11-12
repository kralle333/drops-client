use crate::api::{fetch_games, login};
use crate::client_config::{ClientConfig, Release};
use crate::messages::Message;
use crate::SessionToken;
use iced::Task;

pub fn perform_login(drops_url: &str, username: &str, password: &str) -> Task<Message> {
    Task::perform(
        login(
            drops_url.to_string(),
            username.to_string(),
            password.to_string(),
        ),
        Message::LoggedInFinished,
    )
}

pub fn perform_fetch_games_from_config(config: &ClientConfig) -> Task<Message> {
    let drops_url = config.get_drops_url();
    let session_token = config.get_session_token();
    Task::perform(
        fetch_games(drops_url.to_string(), session_token),
        Message::GamesFetched,
    )
}

async fn play_game(release: Release) -> Release {
    release
}
pub fn perform_play_game(release: Release) -> Task<Message> {
    Task::perform(play_game(release), Message::Run)
}