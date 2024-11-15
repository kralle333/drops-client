use crate::client_config::{Release, ReleaseState};
use anyhow::anyhow;
use self_update::backends::github;
use self_update::{cargo_crate_version, version};
use std::path::PathBuf;

pub fn get_exe_path(
    games_dir: &str,
    game_name_id: &str,
    channel_name: &str,
    version: &str,
) -> PathBuf {
    PathBuf::new()
        .join(games_dir)
        .join(game_name_id)
        .join(&channel_name)
        .join(&version)
}

pub fn newest_release_by_state(
    releases: &[Release],
    channel: Option<&str>,
    state: Option<ReleaseState>,
) -> Option<Release> {
    releases
        .iter()
        .filter(|x| channel.map_or(true, |c| x.channel_name == c))
        .filter(|x| state.as_ref().map_or(true, |s| &x.state == s))
        .max_by(|x, y| x.release_date.cmp(&y.release_date))
        .map(|x| x.clone())
}

pub fn default_platform() -> &'static str {
    if cfg!(windows) {
        return "windows";
    }
    if cfg!(unix) {
        return "linux";
    }
    if cfg!(target_os = "macos") {
        return "mac";
    }
    "unknown"
}

pub fn look_for_newer_version() -> Result<Option<self_update::update::Release>, anyhow::Error> {
    let releases = github::ReleaseList::configure()
        .repo_owner("kralle333")
        .repo_name("drops-client")
        .build()?
        .fetch()?;
    //println!("found releases:");
    //println!("{:#?}\n", releases);

    if releases.is_empty() {
        return Ok(None);
    }

    // Assume first one is latest
    let newer = releases.into_iter().nth(0).unwrap();
    let newer_version = newer.version.to_string();

    let current = cargo_crate_version!();
    if version::bump_is_greater(current, &newer_version).map(|x| !x)? {
        println!("no updates");
        return Err(anyhow!("no update"));
    }

    Ok(Some(newer))
}
