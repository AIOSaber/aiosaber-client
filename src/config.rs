use crate::websocket_handler::{ConfigData, InstallType, WebSocketMod};
use std::sync::Arc;
use tokio::sync::Mutex;
use std::fs::File;
use std::io::{Read, Write};
use yaml_rust::{YamlLoader, Yaml, YamlEmitter};
use log::{debug, info, warn, error};
use std::str::FromStr;
use crate::installer::Installer;
use std::env;
use crate::queue_handler::{DownloadQueueRequest, InstallerQueueRequest, InstallerQueue};
use serde::{Serialize, Deserialize};
use std::path::PathBuf;
use crate::map_index::IndexError;
use crate::beatsaver::{BeatSaverError, BeatSaverMap};
use uuid::Uuid;
use std::collections::HashMap;
use crate::file_watcher::PcMapsWatcher;

#[derive(Clone)]
pub struct LocalData {
    pub installer_queue: tokio::sync::mpsc::Sender<InstallerQueueRequest>,
    pub config: ConfigData,
    pub map_index: Arc<Mutex<MapIndex>>,
}

pub type MapIndex = Vec<MapData>;

#[derive(Clone, Serialize, Deserialize)]
#[serde(untagged)]
pub enum MapData {
    Valid(MapMetadata),
    Unknown(std::path::PathBuf, String),
    Invalid(std::path::PathBuf),
}

#[derive(Clone, Serialize, Deserialize)]
pub struct MapMetadata {
    pub path: std::path::PathBuf,
    pub hash: String,
    pub id: u32,
}

#[derive(Clone)]
pub struct DaemonConfig {
    pub concurrent_downloads: u8,
    current_configs: Arc<Mutex<HashMap<Uuid, LocalData>>>,
    download_queue: tokio::sync::mpsc::Sender<DownloadQueueRequest>,
}

pub enum AuditLogAction {
    MapInstall(BeatSaverMap),
    ModInstall(String),
    MapDelete(u32, String),
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

    fn read_from_file() -> HashMap<Uuid, LocalData> {
        debug!("Reading config from file...");
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
                                let data: LocalData = config.into();
                                let mut inner_data = data.clone();
                                tokio::spawn(async move {
                                    let mutex = inner_data.map_index.lock().await;
                                    let empty = mutex.is_empty();
                                    drop(mutex);
                                    if empty {
                                        inner_data.update_map_index(false).await;
                                    }
                                });
                                vec.push(data);
                            }
                        }
                        DaemonConfig::write_to_file(vec.clone().into_iter()
                            .map(|data| data.config)
                            .collect());
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
        vec.into_iter()
            .map(|local_data| (local_data.config.id, local_data))
            .collect()
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
        emitter.compact(false);
        for config_data in configs {
            let mut hash = yaml_rust::yaml::Hash::new();
            hash.insert(Yaml::String("id".to_owned()), Yaml::String(config_data.id.to_hyphenated().to_string()));
            hash.insert(Yaml::String("restToken".to_owned()), Yaml::String(config_data.rest_token.clone()));
            hash.insert(Yaml::String("installType".to_owned()), Yaml::String(config_data.install_type.to_string()));
            hash.insert(Yaml::String("installLocation".to_owned()), Yaml::String(config_data.install_location.clone()));
            let yaml = Yaml::Hash(hash);
            emitter.dump(&yaml).expect("Failed to write config");
        }

        let mut path = env::current_dir().unwrap().clone();
        path.push("daemon-config.yaml");
        if let Ok(mut file) = File::create(path) {
            file.write_all(out_str.replace("---", "\n---").as_bytes()).ok(); // great lib ngl...
            info!("Done");
        } else {
            error!("An error occurred when writing file to system");
        }
    }

    fn read_map_index_from_file(id: &Uuid) -> Option<MapIndex> {
        debug!("Reading map index from file...");
        let mut path = env::current_dir().unwrap().clone();
        let mut file_name = "map-index-".to_string();
        file_name.push_str(id.to_string().as_str());
        file_name.push_str(".json");
        path.push(file_name);
        if let Ok(data) = std::fs::read(path) {
            if let Ok(map_index) = serde_json::from_slice(data.as_ref()) {
                Some(map_index)
            } else {
                None
            }
        } else {
            None
        }
    }

    fn write_map_index_to_file(id: &Uuid, index: &MapIndex) {
        debug!("Writing map index to file...");
        let value = serde_json::to_vec(index).expect("Failed to parse MapIndex");
        let mut path = env::current_dir().unwrap().clone();
        let mut file_name = "map-index-".to_string();
        file_name.push_str(id.to_string().as_str());
        file_name.push_str(".json");
        path.push(file_name);
        if let Ok(mut file) = File::create(path) {
            file.write_all(value.as_ref()).ok();
        } else {
            error!("An error occurred when writing map index file to system");
        }
    }

    pub async fn update_configs(&self, configs: Vec<ConfigData>) -> Vec<ConfigData> {
        let mut needs_update = Vec::new();
        let mut mutex = self.current_configs.lock().await;
        for config_data in configs {
            for (id, local_data) in mutex.iter_mut() {
                if id.eq(&config_data.id) {
                    if config_data.install_type != local_data.config.install_type ||
                        config_data.install_location.ne(&local_data.config.install_location) {
                        // reset map index if the installation type or location changed
                        needs_update.push(id.clone());
                    }
                    local_data.config = config_data.clone();
                }
            }
            let local: LocalData = config_data.into();
            mutex.insert(local.config.id.clone(), local);
        }
        drop(mutex);
        let configs = self.get_configs().await;
        DaemonConfig::write_to_file(configs.clone());
        for uuid in needs_update {
            let mut mutex = self.current_configs.lock().await;
            if let Some(config) = mutex.get_mut(&uuid) {
                let mut index_lock = config.map_index.lock().await;
                index_lock.clear();
                drop(index_lock);
                config.update_map_index(true).await;
            }
        }
        configs
    }

    pub async fn get_configs(&self) -> Vec<ConfigData> {
        let mutex = self.current_configs.lock().await;
        let mut vec = Vec::new();
        for (_, local_data) in mutex.iter() {
            vec.push(local_data.config.clone());
        }
        vec
    }

    pub async fn get_data(&self) -> Vec<LocalData> {
        let mutex = self.current_configs.lock().await;
        let mut vec = Vec::new();
        for (_, local_data) in mutex.iter() {
            vec.push(local_data.clone());
        }
        vec
    }

    pub async fn queue_map(&self, map: String) -> Result<(), tokio::sync::mpsc::error::SendError<DownloadQueueRequest>> {
        self.download_queue.send(DownloadQueueRequest::Map(map)).await
    }

    pub async fn queue_mod(&self, data: WebSocketMod) -> Result<(), tokio::sync::mpsc::error::SendError<DownloadQueueRequest>> {
        match data {
            WebSocketMod::PcMod(mod_data) => self.download_queue.send(DownloadQueueRequest::PcMod(mod_data)).await,
            WebSocketMod::QuestMod(mod_data) => self.download_queue.send(DownloadQueueRequest::QuestMod(mod_data)).await,
        }
    }
}

impl LocalData {
    pub async fn update_map_index(&mut self, aggressive: bool) -> Option<Vec<IndexError>> {
        match self.config.install_type {
            InstallType::PC => {
                let mut map_index = self.map_index.lock().await;
                let mut buf = PathBuf::from(self.config.install_location.clone());
                buf.push("Beat Saber_Data");
                buf.push("CustomLevels");
                let result = match crate::map_index::index_maps(buf, aggressive).await {
                    Ok(vec) => {
                        let mut errors = Vec::new();
                        for result in vec {
                            let error = match result {
                                Ok((path, hash)) => {
                                    match crate::beatsaver::resolve_map_by_hash(hash.as_str()).await {
                                        Ok(data) => {
                                            map_index.push(MapData::Valid(MapMetadata {
                                                path,
                                                hash,
                                                id: u32::from_str_radix(data.id.as_str(), 16).expect("Map id is not hex, wtf?"),
                                            }))
                                        }
                                        Err(error) => {
                                            match error {
                                                BeatSaverError::RequestError(err, _) => error!("Unexpected request error: {}", err),
                                                BeatSaverError::StatusCodeError(_) => map_index.push(MapData::Unknown(path, hash)),
                                                BeatSaverError::JsonError(err, _, _) => error!("Unexpected json error: {}", err),
                                                BeatSaverError::HttpError(err) => error!("Unexpected http error: {:?}", err)
                                            }
                                        }
                                    }
                                    None
                                }
                                Err(error) => {
                                    match error {
                                        IndexError::NotAMap(_, path) => {
                                            map_index.push(MapData::Invalid(path));
                                            None
                                        }
                                        IndexError::MapJsonError(_, path) => {
                                            map_index.push(MapData::Invalid(path));
                                            None
                                        }
                                        IndexError::InvalidMapInfoDat(path) => {
                                            map_index.push(MapData::Invalid(path));
                                            None
                                        }
                                        IndexError::InvalidDifficulty(_, path) => {
                                            map_index.push(MapData::Invalid(path));
                                            None
                                        }
                                        err => Some(err)
                                    }
                                }
                            };
                            if let Some(error) = error {
                                errors.push(error);
                            }
                        }
                        if errors.len() == 0 {
                            None
                        } else {
                            Some(errors)
                        }
                    }
                    Err(err) => Some(vec![err])
                };
                DaemonConfig::write_map_index_to_file(&self.config.id, &map_index);
                result
            }
            InstallType::Quest => {
                info!("Quest Map index is not supported yet");
                None
            }
        }
    }

    pub async fn rewrite_map_index(&self) {
        debug!("Rewriting map-index file...");
        let mutex = self.map_index.lock().await;
        DaemonConfig::write_map_index_to_file(&self.config.id, &mutex)
    }

    pub async fn is_map_installed(&self, hash: &str) -> bool {
        let vec = self.map_index.lock().await;
        vec.iter().any(|data| data.has_hash(hash))
    }

    pub async fn is_map_installed_by_id(&self, id: &str) -> bool {
        let vec = self.map_index.lock().await;
        vec.iter().any(|data| data.has_id(id))
    }

    pub async fn audit_log_entry(&self, _action: AuditLogAction) {
        // todo
    }
}

impl Into<LocalData> for ConfigData {
    fn into(self) -> LocalData {
        let (installer_queue_tx, installer_queue_rx) = tokio::sync::mpsc::channel(1024);
        let map_index = Arc::new(Mutex::new(DaemonConfig::read_map_index_from_file(&self.id)
            .unwrap_or_default()));
        let data = LocalData {
            installer_queue: installer_queue_tx,
            config: self,
            map_index,
        };
        InstallerQueue::new(installer_queue_rx, data.clone())
            .start(); // todo: can we catch this join handle somehow and make sure it doesnt die
        // one a map/mod is sent into the installer queue, it is hard to track its
        // success state programmatically:
        match data.config.install_type {
            InstallType::PC => {
                PcMapsWatcher::new(data.clone()).start_watcher()
                    .expect("Failed to start maps watcher"); // todo: can we catch this join handle somehow and make sure it doesnt die
            }
            InstallType::Quest => {
                info!("QuestMapsWatcher is not implemented yet!");
            }
        }
        data
    }
}

impl Into<Vec<Installer>> for DaemonConfig {
    fn into(self) -> Vec<Installer> {
        let mut vec = Vec::new();
        for (_, config) in self.current_configs.try_lock().unwrap().iter() {
            vec.push(Installer::from(config.config.clone()));
        }
        vec
    }
}

impl AsRef<PathBuf> for MapData {
    fn as_ref(&self) -> &PathBuf {
        match self {
            MapData::Valid(meta) => &meta.path,
            MapData::Unknown(path, _) => path,
            MapData::Invalid(path) => path
        }
    }
}

impl MapData {
    pub fn has_hash(&self, hash: &str) -> bool {
        match self {
            MapData::Valid(map) => map.hash.eq(hash),
            MapData::Unknown(_, map_hash) => map_hash.eq(hash),
            MapData::Invalid(_) => false
        }
    }
    pub fn has_id(&self, id: &str) -> bool {
        match self {
            MapData::Valid(map) => map.id.eq(&u32::from_str_radix(id, 16).unwrap_or_default()),
            _ => false
        }
    }
}
