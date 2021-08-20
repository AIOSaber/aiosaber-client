use crate::websocket_handler::{ConfigData, InstallType};
use std::sync::Arc;
use tokio::sync::Mutex;
use std::fs::File;
use std::io::{Read, Write};
use yaml_rust::{YamlLoader, Yaml, YamlEmitter};
use log::{info, warn, error};
use std::str::FromStr;
use crate::installer::Installer;
use std::env;
use std::collections::VecDeque;
use crate::queue_handler::{DownloadQueueRequest, InstallerQueueRequest, InstallerQueue};

#[derive(Clone)]
pub struct LocalData {
    pub installer_queue: tokio::sync::mpsc::Sender<InstallerQueueRequest>,
    pub config: ConfigData,
    pub map_queue: VecDeque<String>,
    pub mod_queue: VecDeque<String>,
    pub map_index: MapIndex,
}

#[derive(Clone)]
pub struct MapIndex(Vec<MapMeta>);

#[derive(Clone)]
pub struct MapMeta {
    pub path: std::path::PathBuf,
    pub hash: String,
    pub version: u8,
    pub id: u32,
}

#[derive(Clone)]
pub struct DaemonConfig {
    pub concurrent_downloads: u8,
    current_configs: Arc<Mutex<Vec<LocalData>>>,
    download_queue: tokio::sync::mpsc::Sender<DownloadQueueRequest>,
}

pub enum AuditLogAction {
    MapInstall(String),
    ModInstall(String),
    MapDelete(String),
    ModDelete(String),
}

impl DaemonConfig {
    pub fn new(download_queue: tokio::sync::mpsc::Sender<DownloadQueueRequest>) -> DaemonConfig {
        DaemonConfig {
            concurrent_downloads: 4,
            current_configs: Arc::new(Mutex::new(DaemonConfig::read_from_file())),
            download_queue,
        }
    }

    fn read_from_file() -> Vec<LocalData> {
        let mut path = env::current_dir().unwrap().clone();
        path.push("daemon-config.yaml");
        let mut vec = Vec::new();
        match File::open(path.clone()) {
            Ok(mut file) => {
                let mut contents = String::new();
                file.read_to_string(&mut contents).ok();
                match YamlLoader::load_from_str(contents.as_str()) {
                    Ok(docs) => {
                        for yaml in docs {
                            if let Ok(config) = DaemonConfig::read_yaml_doc(yaml) {
                                vec.push(config.into());
                            }
                        }
                    }
                    Err(error) => {
                        warn!("Invalid yaml configuration: {}", error);
                    }
                }
            }
            Err(err) => {
                warn!("Couldn't open configuration file {}: {}", path.display(), err);
            }
        }
        vec
    }

    fn read_yaml_doc(yaml: Yaml) -> Result<ConfigData, ()> {
        if let Some(map) = yaml.as_hash() {
            let id = map.get(&Yaml::String("id".to_string()))
                .and_then(|yaml| yaml.as_str())
                .map(|str| uuid::Uuid::from_str(str).unwrap())
                .unwrap_or_else(|| uuid::Uuid::new_v4());
            let rest_token = map.get(&Yaml::String("restToken".to_string()))
                .and_then(|yaml| yaml.as_str())
                .map(|str| str.to_string());
            let install_type = map.get(&Yaml::String("installType".to_string()))
                .and_then(|yaml| yaml.as_str())
                .and_then(|str| InstallType::from_str(str).ok());
            let install_location = map.get(&Yaml::String("installLocation".to_string()))
                .and_then(|yaml| yaml.as_str())
                .map(|str| str.to_string());
            if let Some(((rest_token, install_type), install_location)) = rest_token
                .zip(install_type)
                .zip(install_location) {
                return Ok(ConfigData {
                    id,
                    rest_token,
                    install_type,
                    install_location,
                });
            }
        }
        Err(())
    }

    fn write_to_file(configs: Vec<ConfigData>) {
        info!("Writing changed config to file...");
        let mut out_str = String::new();
        let mut emitter = YamlEmitter::new(&mut out_str);
        for config_data in configs {
            let mut hash = yaml_rust::yaml::Hash::new();
            hash.insert(Yaml::String("id".to_owned()), Yaml::String(config_data.id.to_hyphenated().to_string()));
            hash.insert(Yaml::String("restToken".to_owned()), Yaml::String(config_data.rest_token.clone()));
            hash.insert(Yaml::String("installType".to_owned()), Yaml::String(config_data.install_type.to_string()));
            hash.insert(Yaml::String("installLocation".to_owned()), Yaml::String(config_data.install_location.clone()));
            let yaml = Yaml::Hash(hash);
            emitter.dump(&yaml).ok();
        }

        let mut path = env::current_dir().unwrap().clone();
        path.push("daemon-config.yaml");
        if let Ok(mut file) = File::create(path) {
            file.write_all(out_str.as_bytes()).ok();
            info!("Done");
        } else {
            error!("An error occurred when writing file to system");
        }
    }

    pub async fn replace_configs(&self, configs: Vec<ConfigData>) -> Vec<ConfigData> {
        let mut mutex = self.current_configs.lock().await;
        for config_data in configs {
            mutex.iter_mut().for_each(|local_data| {
                if local_data.config.id.eq(&config_data.id) {
                    local_data.config = config_data.clone();
                }
            });
        }
        std::mem::drop(mutex);
        let configs = self.get_configs().await;
        DaemonConfig::write_to_file(configs.clone());
        configs
    }

    pub async fn get_configs(&self) -> Vec<ConfigData> {
        let mutex = self.current_configs.lock().await;
        let mut vec = Vec::new();
        for local_data in mutex.iter() {
            vec.push(local_data.config.clone());
        }
        vec
    }

    pub async fn get_data(&self) -> Vec<LocalData> {
        let mutex = self.current_configs.lock().await;
        let mut vec = Vec::new();
        for local_data in mutex.iter() {
            vec.push(local_data.clone());
        }
        vec
    }

    pub async fn queue_map(&self, map: String) -> Result<(), tokio::sync::mpsc::error::SendError<DownloadQueueRequest>> {
        self.download_queue.send(DownloadQueueRequest::Map(map)).await
    }
}

impl LocalData {
    pub async fn audit_log_entry(&self, _action: AuditLogAction) {
        // todo
    }
}

impl Into<LocalData> for ConfigData {
    fn into(self) -> LocalData {
        let (installer_queue_tx, installer_queue_rx) = tokio::sync::mpsc::channel(1024);
        InstallerQueue::new(installer_queue_rx, self.clone())
            .start(); // todo: can we catch this join handle somehow and make sure it doesnt die
        // one a map/mod is sent into the installer queue, it is hard to track its
        // success state programmatically:
        LocalData {
            installer_queue: installer_queue_tx,
            config: self,
            map_queue: Default::default(),
            mod_queue: Default::default(),
            map_index: MapIndex(Vec::new()),
        }
    }
}

impl Into<Vec<Installer>> for DaemonConfig {
    fn into(self) -> Vec<Installer> {
        let mut vec = Vec::new();
        for config in self.current_configs.try_lock().unwrap().iter() {
            vec.push(Installer::from(config.config.clone()));
        }
        vec
    }
}
