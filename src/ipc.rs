use crate::messages::Message;
use crate::LockFileWithDrop;
use anyhow::{anyhow, Error};
use futures_util::SinkExt;
use iced::futures::Stream;
use iced_futures::{stream, Subscription};
use ipc_channel::ipc::{IpcOneShotServer, IpcSender};
use log::info;
use std::env;

fn handle_other_clients_opening() -> impl Stream<Item = Message> {
    stream::channel(1, |mut output| async move {
        let mut lock_file = LockFileWithDrop::new();
        loop {
            let result = IpcOneShotServer::new();

            let Ok((server, server_name)) = result else {
                info!(
                    "failed to create oneshot server: {}",
                    result.err().unwrap().to_string()
                );
                return;
            };

            info!("created server");
            let msg = lock_file.write_server_name_lock(&server_name);
            if msg.is_err() {
                info!(
                    "Failed to write server name {}: {:?}",
                    server_name,
                    msg.err()
                )
            }

            info!("lets accept the server!");
            let result = tokio::task::spawn_blocking(move || server.accept()).await;
            if let Ok((_, message)) = result.unwrap() {
                info!("Received message: {}", message);
                output
                    .send(Message::IpcArgs(message))
                    .await
                    .expect("failed to send args");
            }
        }
    })
}

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
    if let Some(arg) = args.iter().nth(0) {
        let server_name = LockFileWithDrop::get_server_name();
        if let Ok(sender) = IpcSender::<String>::connect(server_name) {
            sender
                .send(arg.to_string())
                .expect("Failed to send arguments.");
            info!("Arguments sent successfully.");
        } else {
            return Err(anyhow!("Failed to connect to the running instance"));
        }
    }
    Ok(())
}

pub(crate) fn subscription() -> Subscription<Message> {
    Subscription::run_with_id("ipc", handle_other_clients_opening())
}
