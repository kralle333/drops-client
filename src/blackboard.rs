use crate::client_config::{ClientConfig, Game, Release};
use crate::Screen;
use std::path::PathBuf;
use std::process::Command;

#[derive(Default, Clone)]
pub struct Blackboard {
    pub config: ClientConfig,
    pub screen: Screen,
    pub selected_game: Option<Game>,
    pub selected_channel: Option<String>,
    pub selected_version: Option<String>,
    pub is_playing: bool,
}

impl Blackboard {
    pub(crate) fn set_initial_screen(&mut self) {
        self.screen = match self.have_valid_config() {
            true if self.config.has_session_token() => Screen::Main,
            true => Screen::Login,
            false => Screen::Wizard,
        };
    }
}

impl Blackboard {
    pub(crate) fn have_valid_config(&self) -> bool {
        self.config.is_active
    }
}

impl Blackboard {
    pub(crate) fn update_selected_game(&mut self) {
        if self.selected_game.is_none() {
            return;
        }
        let game = self.selected_game.as_ref().unwrap();
        let updated_game = self
            .config
            .get_account_games()
            .iter()
            .find(|x| x.name_id == game.name_id)
            .unwrap()
            .clone();
        self.selected_game = Some(updated_game);
    }

    pub fn run_release(&mut self, game_name_id: &str, release: &Release) {
        let executable_dir = PathBuf::new()
            .join(self.config.get_games_dir())
            .join(game_name_id)
            .join(&release.channel_name)
            .join(&release.version);

        let executable_path = executable_dir.join(&release.executable_path);
        let mut child = Command::new(&executable_path)
            .current_dir(&executable_dir)
            .envs(std::env::vars())
            .spawn()
            .expect(&format!(
                "Failed to run the binary at: {:?}",
                executable_path
            ));

        let _ = child.wait();
        self.is_playing = true;
    }
}
