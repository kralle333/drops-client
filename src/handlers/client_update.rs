use crate::blackboard::Blackboard;
use crate::handlers::MessageHandler;
use crate::messages::Message;
use crate::utils::default_platform;
use crate::{view_utils, Screen};
use anyhow::Context;
use iced::widget::{button, column, row, text, vertical_space};
use iced::{Center, Element, Task};
use self_update::{cargo_crate_version, self_replace};
use std::env;

fn download_newer_version_and_replace(
    release: self_update::update::Release,
) -> Result<(), anyhow::Error> {
    // get the first available release
    let asset = release.asset_for(default_platform(), None).unwrap();

    //info!("creating temp dirs");
    let cur_dir = env::current_dir().context("getting cur dir")?;
    let tmp_dir = tempfile::Builder::new()
        .prefix("self_update")
        .tempdir_in(cur_dir)
        .context("creating temp dir")?;
    let tmp_zip_path = tmp_dir.path().join(&asset.name);
    let tmp_zip = std::fs::File::create(&tmp_zip_path).context("opening zip file")?;

    //info!("downloading");
    self_update::Download::from_url(&asset.download_url)
        .set_header(reqwest::header::ACCEPT, "application/octet-stream".parse()?)
        .download_to(&tmp_zip)?;

    let bin_name_suffix = match default_platform() {
        "windows" => ".exe",
        _ => "",
    };
    //info!("updating!");
    let bin_name = std::path::PathBuf::from(format!("drops-client{}", bin_name_suffix));
    //info!("using binname: {}", bin_name.to_str().unwrap_or(""));
    self_update::Extract::from_source(&tmp_zip_path)
        .archive(self_update::ArchiveKind::Zip)
        .extract_file(tmp_dir.path(), &bin_name)?;
    //info!("replacing!");

    let new_exe = tmp_dir.path().join(bin_name);
    self_replace::self_replace(new_exe)?;

    Ok(())
}

#[derive(Default)]
enum ClientUpdateState {
    #[default]
    HasUpdate,
    IsUpdating,
    UpdateError(String),
    Completed,
}

#[derive(Default)]
pub struct ClientUpdateHandler {
    state: ClientUpdateState,
}

impl ClientUpdateHandler {
    pub fn view(&self, blackboard: &Blackboard) -> Element<Message> {
        match &blackboard.screen {
            Screen::ClientUpdateAvailable(new_release) => match &self.state {
                ClientUpdateState::IsUpdating => {
                    view_utils::container_with_title("Updating!".to_string(), column![])
                }
                ClientUpdateState::UpdateError(e) => view_utils::container_with_title(
                    "Failed to update".to_string(),
                    column![
                        text(e),
                        vertical_space().height(30),
                        button(text("Go to menu").center())
                    ]
                    .align_x(Center)
                    .width(300),
                ),
                ClientUpdateState::Completed => view_utils::container_with_title(
                    "Success!".to_string(),
                    column![
                        text("Please close and open client."),
                        vertical_space().height(30),
                        button(text("close").center()).on_press(Message::CloseClient)
                    ]
                    .align_x(Center)
                    .width(300),
                ),
                ClientUpdateState::HasUpdate => {
                    let buttons_row = row![]
                        .push(
                            button(text("cancel").size(16).center())
                                .on_press(Message::GoToInitialScreen),
                        )
                        .push(
                            button(text("update").size(16).center())
                                .on_press(Message::UpdateClient(new_release.clone())),
                        )
                        .spacing(20);

                    let content = column![]
                        .push(
                            text(format!(
                                "{} -> {}",
                                cargo_crate_version!(),
                                new_release.version
                            ))
                            .size(32),
                        )
                        .push(vertical_space().height(30))
                        .push(buttons_row);
                    view_utils::container_with_title("New version available!".to_string(), content)
                }
            },
            _ => column![].into(),
        }
    }
}

impl MessageHandler for ClientUpdateHandler {
    fn update(&mut self, message: Message, _: &mut Blackboard) -> Task<Message> {
        match message {
            Message::UpdateClient(release) => {
                self.state = ClientUpdateState::IsUpdating;
                let result = download_newer_version_and_replace(release);
                match result {
                    Ok(_) => {
                        self.state = ClientUpdateState::Completed;
                    }
                    Err(e) => {
                        self.state = ClientUpdateState::UpdateError(e.to_string());
                    }
                }
                Task::none()
            }
            _ => Task::none(),
        }
    }
}
