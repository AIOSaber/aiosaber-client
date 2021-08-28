mod webserver;
mod websocket_handler;
mod one_click;
mod config;
mod beatsaver;
mod installer;
mod map_index;
mod queue_handler;
mod file_watcher;
mod http_client;

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
use curl::easy::Easy;
use std::path::PathBuf;
use crate::queue_handler::DownloadQueueHandler;

#[cfg(not(target_family = "windows"))]
#[global_allocator]
static GLOBAL: Jemalloc = Jemalloc;

#[tokio::main]
async fn main() {
    env_logger::init_from_env(Env::new().default_filter_or("info"));

    env::set_current_dir(env::current_exe().unwrap().parent().unwrap()).ok();

    if env::args().len() > 1 {
        let operator: String = env::args().nth(1).unwrap();

        if operator.eq("--watcher") {
            let (tx, _) = tokio::sync::mpsc::channel(1);
            for data in DaemonConfig::new(tx).get_data().await {
                if data.config.install_type == crate::websocket_handler::InstallType::PC {
                    crate::file_watcher::PcMapsWatcher::new(data).start_watcher().unwrap().await.ok();
                }
            }
            return;
        }

        if operator.eq("--test-hash") {
            if env::args().len() != 3 {
                error!("--test-hash takes exactly one extra argument");
            } else {
                let dir = env::args().nth(2).unwrap();
                info!("Dir is: {}", dir.as_str());
                let calculated_hash = map_index::generate_hash(
                    std::path::PathBuf::from_str(dir.as_str()).expect("Path is not a dir")
                ).expect("Invalid directory");
                info!("Hash is: {}", calculated_hash.as_str());
                let map = beatsaver::resolve_map_by_hash(calculated_hash.as_str()).await.expect("Map not found!");
                info!("BeatSaver Map is: {} ({} - {})", map.id, map.metadata.song_name, map.metadata.level_author_name);
            }
            return;
        }

        if operator.eq("--scan-maps") {
            if env::args().len() != 4 {
                error!("--scan-maps <--aggressive/--relaxed> <path>");
            } else {
                let aggressive = env::args().nth(2).unwrap().eq("--aggressive");
                let dir = env::args().nth(3).unwrap();
                let path = std::path::PathBuf::from_str(dir.as_str())
                    .expect("Path is not a dir");
                let start = std::time::SystemTime::now();
                let results = map_index::index_maps(path, aggressive).await.expect("Cannot index");
                let duration = start.elapsed().unwrap().as_millis();
                info!("Indexing took: {}ms", duration);
                let size = results.len();
                let data = results.iter()
                    .filter_map(|result| result.as_ref().ok())
                    .map(|(buf, hash)| (hash.clone(), buf.clone()))
                    .collect::<Vec<(String, std::path::PathBuf)>>();
                let mut found = Vec::new();
                let mut dup = false;
                for (hash, path) in data {
                    let dup_path = found.clone().into_iter()
                        .find_map(|entry: (String, PathBuf)| {
                            if entry.0.eq(&hash) {
                                Some(entry.1)
                            } else {
                                None
                            }
                        });
                    if let Some(dup_path) = dup_path {
                        warn!("Duplicate hash: {} @ {}\nFirst found here: {}",
                              hash.as_str(), path.display(), dup_path.display());
                        dup = true;
                        continue;
                    }
                    found.push((hash, path));
                }
                if !dup {
                    info!("No duplicates found!");
                }
                let err = results.into_iter()
                    .filter_map(|result| result.err())
                    .collect::<Vec<map_index::IndexError>>();
                info!("Total: {} - OK: {} - Failure: {}", size, size - err.len(), err.len());
                err.into_iter()
                    .for_each(|err| error!("{:?}", err));
            }
            return;
        }

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
                info!("Adding map {} to install queue...", hash.as_str());
                match installer::push_map_to_install_queues(hash).await {
                    Ok(_) => info!("Success!"),
                    Err(err) => {
                        error!("Failure: {:?}", err);
                        tokio::time::sleep(std::time::Duration::from_secs(2)).await;
                    }
                }
            }
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

    let version = env!("CLIENT_VERSION").to_string();

    let (queue_handler_tx, queue_handler_rx) = tokio::sync::mpsc::channel(1024);
    let config = DaemonConfig::new(queue_handler_tx);
    let (web_server, socket_handler) = WebServer::create_server(version, config.clone())
        .start(SocketAddr::new(IpAddr::from_str("127.0.0.1").unwrap(), 2706));
    let websocket_sender = socket_handler.get_sender();
    let websocket_handle = socket_handler.start();
    let queue_handle = DownloadQueueHandler::new(queue_handler_rx, config, websocket_sender).start();

    tokio::select! {
        _val = web_server => {
            warn!("Webserver died. Restarting!");
            exit(1);
        }
        _val = websocket_handle => {
            warn!("WebSocket Handler died. Restarting!");
            exit(1);
        }
        _val = queue_handle => {
            warn!("Download Queue Handler died. Restarting!");
            exit(1);
        }
    }
}

pub mod built_info {
    include!(concat!(env!("OUT_DIR"), "/built.rs"));
}