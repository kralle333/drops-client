use crate::client_config::ReleaseState::Installed;
use crate::downloading::{Download, DownloadProgress, DownloadState};
use crate::handlers::MessageHandler;
use crate::messages::Message;
use crate::blackboard::Blackboard;
use iced::Task;
use iced_futures::Subscription;
use log::error;

#[derive(Default)]
pub struct DownloadMessageHandler {
    pub(crate) downloads: Vec<Download>,
}

impl DownloadMessageHandler {
    pub(crate) fn subscription(&self) -> Subscription<Message> {
        Subscription::batch(self.downloads.iter().map(Download::subscription))
    }
}

impl MessageHandler for DownloadMessageHandler {
    fn update(&mut self, message: Message, blackboard: &mut Blackboard) -> Task<Message> {
        match message {
            Message::Download(game, release) => {
                self.downloads
                    .push(Download::new(&release, &game, &blackboard.config));
            }
            Message::DownloadProgressing((id, Ok(progress))) => match progress {
                DownloadProgress::Downloading { percent } => {
                    self.downloads
                        .iter_mut()
                        .find(|x| x.game_name_id == id)
                        .unwrap()
                        .state = DownloadState::Downloading {
                        progress_percentage: percent,
                    }
                }
                DownloadProgress::Finished { release } => {
                    if let Err(_) = blackboard.config.update_install_state(
                        &release.game_name_id,
                        &release.version,
                        &release.channel_name,
                        Installed,
                    ) {
                        //TODO: Display error
                        error!("failed to update config install state");
                    }
                    blackboard.config.save().expect("failed to save config!");
                    blackboard.update_selected_game();

                    self.downloads.retain(|x| x.game_name_id != id);
                }
            },
            Message::DownloadProgressing((id, Err(error))) => {
                self.downloads
                    .iter_mut()
                    .find(|x| x.game_name_id == id)
                    .unwrap()
                    .state = DownloadState::Errored(error)
            }
            Message::CloseDownloadError(id) => {
                self.downloads.retain(|x| x.game_name_id != id);
            }
            _ => {
                error!("invalid download state!")
            }
        }
        Task::none()
    }
}
