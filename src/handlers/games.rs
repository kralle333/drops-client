use crate::blackboard::Blackboard;
use crate::handlers::MessageHandler;
use crate::messages::Message;
use iced::Task;
use log::error;

#[derive(Default)]
pub struct GamesMessageHandler {}

impl MessageHandler for GamesMessageHandler {
    fn update(&mut self, message: Message, blackboard: &mut Blackboard) -> Task<Message> {
        match message {
            Message::SelectGame(game) => {
                blackboard.selected_channel = match game.selected_channel.as_ref() {
                    None => game.releases.first().map(|x| x.channel_name.to_string()),
                    Some(channel) => Some(channel.to_string()),
                };
                blackboard.selected_game = Some(game);
            }

            Message::Run(release) => {
                let game_name_id = &blackboard
                    .selected_game
                    .as_ref()
                    .unwrap()
                    .name_id
                    .to_string();
                blackboard.run_release(game_name_id, &release)
            }
            _ => {
                error!("Unexpected state!")
            }
        }
        Task::none()
    }
}
