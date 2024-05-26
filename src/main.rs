mod throttle;
mod watcher;

use crate::watcher::LogWatcher;
use log::{error, info, trace};
use rcon::{Connection, Error};
use std::env::var;
use std::fs::OpenOptions;
use std::path::PathBuf;
use steamlocate::SteamDir;
use throttle::Throttler;
use tokio::net::TcpStream;
use tokio::time::Duration;

#[derive(PartialEq, Debug)]
enum ConsoleEvent {
    Record,
    Stop,
}

impl ConsoleEvent {
    fn from_chat(chat: &str) -> Option<Self> {
        if chat.contains("[SOAP] Soap DM unloaded.") | chat.contains("[P-REC] Recording...") {
            Some(ConsoleEvent::Record)
        } else if chat.contains("[LogsTF] Uploading logs...")
            | chat.contains("[P-REC] Stop record.")
        {
            Some(ConsoleEvent::Stop)
        } else if chat.contains("(Demo Support) End recording") {
            let tf_path: PathBuf = log_path().parent().unwrap().to_path_buf();
            let demo_path: &str = &chat.split(" ").collect::<Vec<&str>>()[4];
            let path = tf_path.join(demo_path);
            info!("Found demo: {}", &path.display());
            highlights::demo::get_highlights(&path);
            None
        } else {
            None
        }
    }

    fn command(&self) -> &'static str {
        match self {
            ConsoleEvent::Record => "ds_record",
            ConsoleEvent::Stop => "ds_stop",
        }
    }

    async fn send(&self, rcon_password: &str) {
        let builder = Connection::<TcpStream>::builder()
            .connect("127.0.0.1:27015", rcon_password)
            .await;
        let Ok(mut conn) = builder else {
            error!("Failed to connect to rcon");
            return;
        };
        if let Err(e) = conn.cmd(self.command()).await {
            error!("Error while sending rcon event: {e:?}")
        }
        info!("Sending {:?}", self);
    }
}

#[tokio::main]
async fn main() -> Result<(), Error> {
    env_logger::init();
    info!("P-REC started");

    let rcon_password = var("RCON_PASSWORD").unwrap_or_else(|_| "prec".to_string());

    let path = log_path();

    // make sure the file exists
    OpenOptions::new().write(true).create(true).open(&path)?;

    let log_watcher = LogWatcher::new(path);

    let delay = Duration::from_millis(7500);
    let mut throttler = Throttler::new(delay);

    for line_result in log_watcher {
        let line = line_result?;
        trace!("got log line: {line}");
        if let Some(event) = ConsoleEvent::from_chat(line.trim()) {
            if let Some(event) = throttler.debounce(event) {
                event.send(&rcon_password).await;
            }
        }
    }

    Ok(())
}

fn log_path() -> PathBuf {
    let dir = SteamDir::locate().unwrap();
    dir.path()
        .join("steamapps")
        .join("common")
        .join("Team Fortress 2")
        .join("tf")
        .join("console.log")
}
