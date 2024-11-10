use crate::client_config::{ClientConfig, Game, Release};
use crate::downloading::{DownloadError, DownloadProgress};
use crate::errors::{ConfigError, FetchGamesError, LoginError};
use crate::SessionToken;
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
    Install(Game, Release),
    SelectGamesDir,
    UsernameChanged(String),
    PasswordChanged(String),
    DropsUrlChanged(String),
    TestDropsUrl,
    WizardCanReachHostChecked(Result<(), String>),
    ChannelChanged(String),
    ServerChanged(String),
    FinishWizard,
    GoToWizard,
    GoToLogin,
    DownloadProgressing((String, Result<DownloadProgress, DownloadError>)),
    CloseDownloadError(String),
    Logout,
}
