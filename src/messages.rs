use crate::client_config::{ClientConfig, Game, Release, SessionToken};
use crate::errors::{ConfigError, FetchGamesError, LoginError};
use crate::handlers::download::{DownloadError, DownloadProgress, DownloadRequest};
use crate::ipc::Event;
use crate::Screen;
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
    Download(DownloadRequest),

    UsernameChanged(String),
    PasswordChanged(String),
    DropsUrlChanged(String),
    TestDropsUrl,

    WizardCanReachHostChecked(Result<(), String>),
    SelectedChannelChanged(String),
    ServerChanged(String),
    SelectGamesDir,
    FinishWizard,

    GoToScreen(Screen),
    GoToInitialScreen,
    UpdateClient(self_update::update::Release),
    DownloadProgressing((String, Result<DownloadProgress, DownloadError>)),
    CloseDownloadError(String),
    Logout,
    ClearRequestedGameToPlay,
    SelectedVersionChanged(String),
    CloseError,
    Ipc(Event),
}
