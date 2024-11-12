use crate::client_config::{ClientConfig, Game, Release};
use crate::downloading::{DownloadError, DownloadProgress};
use crate::errors::{ConfigError, FetchGamesError, LoginError};
use crate::{Screen, SessionToken};
use drops_messages::requests::GetGamesResponse;

#[derive(Debug, Clone)]
pub enum Message {
    ConfigOpened(Result<ClientConfig, ConfigError>),
    Login,
    LoggedInFinished(Result<SessionToken, LoginError>),
    FetchGames,
    GamesFetched(Result<GetGamesResponse, FetchGamesError>),

    SelectGame(Game),
    Run(Release),
    Download(Game, Release),

    UsernameChanged(String),
    PasswordChanged(String),
    DropsUrlChanged(String),
    TestDropsUrl,

    WizardCanReachHostChecked(Result<(), String>),
    ChannelChanged(String),
    ServerChanged(String),
    SelectGamesDir,
    FinishWizard,

    GoToScreen(Screen),
    DownloadProgressing((String, Result<DownloadProgress, DownloadError>)),
    CloseDownloadError(String),
    Logout,
    ClearRequestedGameToPlay,
}
