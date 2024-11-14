use crate::blackboard::Blackboard;
use crate::messages::Message;
use iced::Task;

pub mod client_update;
pub mod download;
pub mod games;
pub mod login;
pub mod wizard;

pub trait MessageHandler {
    fn update(&mut self, message: Message, blackboard: &mut Blackboard) -> Task<Message>;
}
