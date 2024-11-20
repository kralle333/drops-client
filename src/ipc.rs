use crate::client_config::get_config_dir;
use crate::messages::Message;
use anyhow::{anyhow, Context, Error};
use fs2::FileExt;
use futures_util::SinkExt;
use iced::futures::Stream;
use iced_futures::{stream, Subscription};
#[cfg(windows)]
use interprocess::os::windows::local_socket::NamedPipe;
use ipmb::{label, RecvError};
use log::{debug, error, info, warn};
use std::fs::{File, OpenOptions};
use std::io::{Read, Write};
use std::path::PathBuf;
use std::time::Duration;
use std::{env, fs};
use sysinfo::Pid;

pub struct LockFileWithDrop {
    lock_file: File,
    path: PathBuf,
}

#[allow(unused)]
#[derive(Debug, Clone)]
pub enum Event {
    ArgsReceived(String),
    Yield,
}

impl LockFileWithDrop {
    pub fn new() -> Result<Box<Self>, Error> {
        let mut file = OpenOptions::new()
            .read(true)
            .write(true)
            .create(true)
            .truncate(true)
            .open(Self::lock_file_path())
            .context("Failed to open or create lock file.")?;

        file.write_all(format!("{}", std::process::id()).as_bytes())
            .context("failed to write process id to lock file")?;
        file.lock_exclusive().context("Failed to lock lock file.")?;
        Ok(Box::new(Self {
            lock_file: file,
            path: Self::lock_file_path(),
        }))
    }

    pub fn read_lock() -> Option<Pid> {
        if Self::lock_file_path().exists() {
            let mut pid = "".to_string();
            let Ok(mut file) = File::open(Self::lock_file_path()) else {
                warn!("failed to open lock file");
                return None;
            };
            let Ok(_) = file.read_to_string(&mut pid) else {
                warn!("failed to read file content {}", pid);
                return None;
            };
            let Ok(pid) = pid.parse() else {
                warn!("failed to parse pid: {}", pid);
                return None;
            };
            return Some(pid);
        }
        None
    }

    fn lock_file_path() -> PathBuf {
        get_config_dir().join("drops.lock")
    }
}

impl Drop for LockFileWithDrop {
    fn drop(&mut self) {
        let unlock_result = self.lock_file.unlock();
        if unlock_result.is_err() {
            error!("Failed to unlock lock file")
        }
        fs::remove_file(&self.path).expect("Failed to delete lock file")
    }
}

#[allow(unused)]
fn handle_other_clients_opening() -> impl Stream<Item = Event> {
    stream::channel(1, |mut output| async move {
        let options = ipmb::Options::new("drops-client", label!("server"), "");
        let (_, receiver) =
            ipmb::join::<String, String>(options, None).expect("failed to setup ipc server");
        let mut receiver = receiver;
        loop {
            if let Err(_) = output.send(Event::Yield).await {
                debug!("failed to send field!");
            }

            match receiver.recv(Some(Duration::from_micros(5))) {
                Ok(message) => {
                    debug!("received message!!!");
                    if let Err(e) = output.send(Event::ArgsReceived(message.payload)).await {
                        debug!("failed to output message: {}", e);
                    }
                }
                Err(e) => match e {
                    RecvError::Timeout => {}
                    _ => {
                        debug!("got bad err!");
                    }
                },
            }
        }
    })
}

#[allow(unused)]
pub(crate) fn try_send_args() -> Result<(), Error> {
    let args: Vec<String> = env::args().skip(1).collect();
    if args.len() > 1 {
        return Err(anyhow!("invalid number of arguments!"));
    }
    // no arguments, just ignore
    if args.len() != 1 {
        return Ok(());
    }

    // let's send this argument to the running instance
    if let Some(arg) = args.get(0) {
        let options = ipmb::Options::new("drops-client", label!("client"), "");
        let (sender, _) =
            ipmb::join::<String, String>(options, None).expect("failed to setup ipc server");
        let selector = ipmb::Selector::unicast("server");
        sender.send(ipmb::Message::new(selector, arg.to_string()))?;
        info!("args sent successfully to running client");
    }
    Ok(())
}

#[allow(unused)]
pub(crate) fn subscription() -> Subscription<Message> {
    Subscription::run(handle_other_clients_opening).map(Message::Ipc)
}
