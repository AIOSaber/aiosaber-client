mod webserver;
mod websocket_handler;
mod one_click;
mod config;
mod beatsaver;
mod installer;

#[cfg(not(target_family = "windows"))]
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
use curl::easy::Easy;

#[cfg(not(target_family = "windows"))]
#[global_allocator]
static GLOBAL: Jemalloc = Jemalloc;

#[tokio::main]
async fn main() {
    env_logger::init_from_env(Env::new().default_filter_or("info"));

    env::set_current_dir(env::current_exe().unwrap().parent().unwrap()).ok();

    if env::args().len() > 1 {
        let operator: String = env::args().nth(1).unwrap();

        if operator.eq("--test-adb") {
            match installer::execute_adb("adb".to_owned(), vec!["version"]) {
                Ok(_) => info!("ADB found & successfully executed"),
                Err(err) => if let Some(error) = err {
                    error!("Couldn't start command: {}", error);
                    error!("If it couldn't find the file, it means ADB is not properly installed and/or not in PATH.");
                } else {
                    error!("ADB version call failed");
                }
            };
            return;
        }

        if operator.eq("--test-curl") {
            let mut easy = Easy::new();
            easy.url("https://ipinfo.io").unwrap();
            easy.perform().unwrap();

            info!("{}", easy.response_code().unwrap());
            return;
        }

        if operator.eq("--dry-run") {
            return;
        }

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

                match beatsaver::resolve_map_by_id(hash).await {
                    Ok(map) => {
                        if let Ok((version, data)) = installer::retrieve_map_data(&map).await {
                            let installers: Vec<Installer> = DaemonConfig::new().into();
                            let mut futures = Vec::new();
                            let mut tasks = Vec::new();
                            if installers.len() == 1 {
                                // Yes duplicate code, to save memory expensive clones on a single installer, which should be most of the users
                                match installers.get(0).unwrap() {
                                    Installer::PC(installer) => {
                                        installer.install_map(map, data.as_ref());
                                    }
                                    Installer::Quest(installer) => {
                                        if let Some(future) = installer.install_map(version.clone(), data.clone()) {
                                            futures.push(future);
                                            tasks.push((installer.clone(), version, data))
                                        }
                                    }
                                }
                            } else {
                                for installer in installers {
                                    match installer {
                                        Installer::PC(installer) => {
                                            installer.install_map(map.clone(), data.as_ref());
                                        }
                                        Installer::Quest(installer) => {
                                            if let Some(future) = installer.install_map(version.clone(), data.clone()) {
                                                futures.push(future);
                                                tasks.push((installer.clone(), version.clone(), data.clone()))
                                            }
                                        }
                                    }
                                }
                            }
                            info!("Awaiting async tasks...");
                            let results = futures_util::future::join_all(futures).await;
                            for i in 0..results.len() {
                                if let Ok(result) = results.get(i).unwrap() {
                                    if let Err(_err) = result {
                                        let (_installer, version, _data) = tasks.get(i).unwrap();
                                        error!("Task download errored: {}", version.hash);
                                    }
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