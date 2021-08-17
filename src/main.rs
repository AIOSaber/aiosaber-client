mod webserver;
mod websocket_handler;
mod one_click;
mod config;
mod beatsaver;
mod installer;

#[cfg(not(target_env = "msvc"))]
use jemallocator::Jemalloc;
use env_logger::Env;
use crate::webserver::WebServer;
use std::net::{SocketAddr, IpAddr};
use std::str::FromStr;
use log::{info, warn, error};
use std::process::exit;
use std::env;
use crate::config::DaemonConfig;
use crate::installer::Installer;

#[cfg(not(target_env = "msvc"))]
#[global_allocator]
static GLOBAL: Jemalloc = Jemalloc;

#[tokio::main]
async fn main() {
    env_logger::init_from_env(Env::new().default_filter_or("info"));

    env::set_current_dir(env::current_exe().unwrap().parent().unwrap()).ok();

    if env::args().len() > 1 {
        let operator: String = env::args().nth(1).unwrap();
        if operator.eq("--privileged-one-click") {
            crate::one_click::privileged_setup();
            return;
        }

        if operator.eq("--map-install") {
            if env::args().len() != 3 {
                error!("--map-install takes exactly one extra argument");
            } else {
                let mut hash = env::args().nth(2).unwrap();
                if hash.starts_with("aiosaber://") {
                    hash = hash.replace("aiosaber://", "");
                }
                if hash.ends_with("/") {
                    hash.remove(hash.len() - 1);
                }

                info!("Installing map {}", hash.as_str());

                match beatsaver::resolve_download_url(hash).await {
                    Ok((download_url, folder_name)) => {
                        info!("Found download url {}", download_url.as_str());
                        let installers: Vec<Installer> = DaemonConfig::new().into();
                        for installer in installers {
                            match installer {
                                Installer::PC(installer) => {
                                    installer.install_map(folder_name.clone(), download_url.clone()).await;
                                },
                                Installer::Quest(_installer) => {
                                    info!("Lol quest unsupported")
                                }
                            }
                        }
                    }
                    Err(error_message) => {
                        error!("An error occurred during download: {}", error_message);
                    }
                }
            }
            info!("This window automatically closes in a few seconds!");
            tokio::time::sleep(std::time::Duration::from_secs(5)).await;
            return;
        }
    }

    let version: Option<&str> = built_info::GIT_COMMIT_HASH;
    let dirty: Option<bool> = built_info::GIT_DIRTY;
    let profile: &str = built_info::PROFILE;
    let build_time: &str = built_info::BUILT_TIME_UTC;
    info!(
        "Starting aiosaber-client with revision {} ({}), built with profile {} at {}",
        version.unwrap_or("{untagged build}"),
        if dirty.unwrap_or(true) {
            "dirty"
        } else {
            "clean"
        },
        profile,
        build_time
    );


    let config = DaemonConfig::new();
    let (web_server, socket_handler) = WebServer::create_server(config).start(
        SocketAddr::new(IpAddr::from_str("127.0.0.1").unwrap(), 2706));
    let _websocket_sender = socket_handler.get_sender();
    let websocket_handle = socket_handler.start();

    tokio::select! {
        _val = web_server => {
            warn!("Webserver died. Restarting!");
            exit(1);
        }
        _val = websocket_handle => {
            warn!("WebSocket Handler died. Restarting!");
            exit(1);
        }
    }
}

pub mod built_info {
    include!(concat!(env!("OUT_DIR"), "/built.rs"));
}