use crate::messages::Message;
use crate::blackboard::Blackboard;
use iced::Task;

pub mod download;
pub mod games;
pub mod login;
pub mod wizard;

pub trait MessageHandler {

    fn update(&mut self, message: Message, blackboard: &mut Blackboard) -> Task<Message>;
}
