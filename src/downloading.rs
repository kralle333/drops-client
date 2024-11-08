use crate::api::{unzip_file, InstalledRelease};
use crate::client_config::{ClientConfig, Game, Release};
use crate::messages::Message;
use crate::{default_platform, SessionToken};
use futures_util::{SinkExt, Stream, StreamExt};
use iced_futures::stream::try_channel;
use iced_futures::Subscription;
use log::error;
use std::fs;
use std::io::Cursor;
use std::path::PathBuf;
use std::sync::Arc;
use zip::ZipArchive;

#[derive(Debug, Clone)]
pub enum DownloadError {
    RequestFailed(Arc<reqwest::Error>),
    IoError,
}
impl From<reqwest::Error> for DownloadError {
    fn from(error: reqwest::Error) -> Self {
        DownloadError::RequestFailed(Arc::new(error))
    }
}

#[derive(Debug, Clone)]
pub enum DownloadProgress {
    Downloading { percent: f32 },
    Finished { release: InstalledRelease },
}

#[derive(Debug, Clone)]
pub struct Download {
    pub(crate) game_name_id: String,
    game_dir: String,
    url: String,
    session_token: SessionToken,
    version: String,
    channel_name: String,
    size_bytes: u64,
    pub(crate) state: DownloadState,
}

impl Download {
    pub fn new(release: &Release, game: &Game, config: &ClientConfig) -> Self {
        Self {
            game_name_id: game.name_id.to_string(),
            game_dir: config.get_games_dir(),
            url: config.get_drops_url(),
            session_token: SessionToken(config.get_session_token()),
            version: release.version.to_string(),
            channel_name: release.channel_name.to_string(),
            state: DownloadState::Downloading {
                progress_percentage: 0.0,
            },
            size_bytes: release.size_bytes,
        }
    }

    pub fn download(&self) -> impl Stream<Item = Result<DownloadProgress, DownloadError>> {
        let url = format!(
            "{}/releases/{}/{}/{}/{}",
            self.url,
            self.game_name_id,
            default_platform(),
            self.channel_name,
            self.version
        );

        let output_dir = PathBuf::new()
            .join(&self.game_dir)
            .join(&self.game_name_id)
            .join(&self.channel_name)
            .join(&self.version);
        let token = self.session_token.0.to_string();
        let release = InstalledRelease {
            game_name_id: self.game_name_id.to_string(),
            version: self.version.to_string(),
            channel_name: self.channel_name.to_string(),
        };
        let content_length = self.size_bytes;
        try_channel(1, move |mut output| async move {
            let _ = output
                .send(DownloadProgress::Downloading { percent: 0.0 })
                .await;
            let client = crate::api::build_client();
            let response = client.get(&url).header("cookie", token).send().await?;

            let stream = response.bytes_stream();
            tokio::pin!(stream); // Pin the stream for iteration
            let mut downloaded = 0;
            let total = content_length;
            let mut zip_data = Vec::new();
            while let Some(Ok(chunk)) = stream.next().await {
                downloaded += chunk.len();
                zip_data.extend_from_slice(&chunk);
                let percent = 100.0 * (downloaded as f32 / total as f32);
                let _ = output.send(DownloadProgress::Downloading { percent }).await;
            }

            let reader = Cursor::new(zip_data);
            let mut zip = ZipArchive::new(reader).map_err(|_| DownloadError::IoError)?;

            fs::create_dir_all(&output_dir).expect("failed creating unzip folder");
            let output_dir = output_dir.as_path().to_str().unwrap();
            unzip_file(&mut zip, output_dir).map_err(|e| {
                error!("Failed to unzip file: {}", e);
                DownloadError::IoError
            })?;

            output
                .send(DownloadProgress::Finished { release })
                .await
                .map_err(|_| DownloadError::IoError)?;

            Ok(())
        })
    }

    pub fn subscription(&self) -> Subscription<Message> {
        match self.state {
            DownloadState::Downloading { .. } => {
                let id = self.game_name_id.to_string();
                Subscription::run_with_id(
                    id.to_string(),
                    self.download()
                        .map(move |progress| (id.to_string(), progress)),
                )
                .map(Message::DownloadProgressing)
            }
            _ => Subscription::none(),
        }
    }
}

#[derive(Debug, Clone)]
pub enum DownloadState {
    Idle,
    Downloading { progress_percentage: f32 },
    Errored(DownloadError),
}
