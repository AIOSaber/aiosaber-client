use crate::config::{DaemonConfig, LocalData};
use tokio::sync::mpsc::Receiver;
use tokio::task::JoinHandle;
use tokio::sync::Semaphore;
use log::{info, error};
use crate::beatsaver;
use crate::beatsaver::{MapVersion, BeatSaverMap};
use crate::websocket_handler::{WebSocketHandler, WebSocketMessage, ResultMsg, ConfigData, ResultMessageData};
use crate::websocket_handler::ResultMessageData::MapInstallError;
use crate::installer::Installer;
use std::sync::Arc;
use thiserror::Error;

pub enum DownloadQueueRequest {
    Map(String)
}

pub struct DownloadQueueHandler {
    receiver: Receiver<DownloadQueueRequest>,
    config: DownloadQueueHandlerConfiguration,
}

#[derive(Clone)]
pub struct DownloadQueueHandlerConfiguration {
    config: DaemonConfig,
    websocket: tokio::sync::broadcast::Sender<warp::ws::Message>,
}

impl DownloadQueueHandler {
    pub fn new(receiver: Receiver<DownloadQueueRequest>, config: DaemonConfig, websocket: tokio::sync::broadcast::Sender<warp::ws::Message>) -> DownloadQueueHandler {
        DownloadQueueHandler {
            receiver,
            config: DownloadQueueHandlerConfiguration {
                config,
                websocket,
            },
        }
    }

    fn handle_install_result(config: ConfigData, receiver: tokio::sync::oneshot::Receiver<InstallerQueueResult>,
                             websocket: tokio::sync::broadcast::Sender<warp::ws::Message>) -> JoinHandle<()> {
        tokio::spawn(async move {
            match receiver.await {
                Ok(result) => {
                    match result {
                        InstallerQueueResult::Success(map, version) => {
                            WebSocketHandler::send_static(websocket, WebSocketMessage::ResultResponse(ResultMsg {
                                action: "InstallMaps".to_string(),
                                success: true,
                                data: ResultMessageData::MapInstallSuccess(config.id, map.id, version.hash),
                            }))
                        }
                        InstallerQueueResult::Error(map, _, error) => {
                            WebSocketHandler::send_static(websocket, WebSocketMessage::ResultResponse(ResultMsg {
                                action: "InstallMaps".to_string(),
                                success: false,
                                data: ResultMessageData::MapInstallError(Some(config.id), map.id, error.to_string()),
                            }))
                        }
                        InstallerQueueResult::AlreadyInstalled(map, version) => {
                            info!("Map {} ({}) was already installed... Skipping", map.id, version.hash);
                        }
                    }
                }
                Err(err) => {
                    error!("No result was received: {}", err);
                }
            }
        })
    }

    async fn download_map(config: DownloadQueueHandlerConfiguration, id: String) {
        match beatsaver::resolve_map_by_id(id.clone()).await {
            Ok(map) => {
                match beatsaver::retrieve_map_data(&map).await {
                    Ok((version, data)) => {
                        let installers = config.config.get_data().await;
                        if installers.is_empty() {
                            error!("No installers configured");
                            return;
                        }
                        if installers.len() == 1 {
                            let (tx, rx) = tokio::sync::oneshot::channel();
                            let installer = installers.first().unwrap();
                            if let Some(err) = installer.installer_queue
                                .send(InstallerQueueRequest::create(tx, InstallerQueueData::Map(map, version, data)))
                                .await
                                .err() {
                                error!("Failed to send map data to installer: {}", err);
                            } else {
                                DownloadQueueHandler::handle_install_result(installer.config.clone(), rx, config.websocket.clone());
                            }
                        } else {
                            for installer_data in installers {
                                let (tx, rx) = tokio::sync::oneshot::channel();
                                if let Some(err) = installer_data.installer_queue
                                    .send(InstallerQueueRequest::create(tx, InstallerQueueData::Map(map.clone(), version.clone(), data.clone())))
                                    .await
                                    .err() {
                                    error!("Failed to send map data to installer: {}", err);
                                } else {
                                    DownloadQueueHandler::handle_install_result(installer_data.config.clone(), rx, config.websocket.clone());
                                }
                            }
                        }
                    }
                    Err(error) => {
                        error!("BeatSaverDownloadError: {}", error);
                        WebSocketHandler::send_static(config.websocket.clone(), WebSocketMessage::ResultResponse(ResultMsg {
                            action: "InstallMaps".to_string(),
                            success: false,
                            data: MapInstallError(None, id, error.to_string()),
                        }))
                    }
                }
            }
            Err(error) => {
                error!("BeatSaverError: {}", error);
                WebSocketHandler::send_static(config.websocket.clone(), WebSocketMessage::ResultResponse(ResultMsg {
                    action: "InstallMaps".to_string(),
                    success: false,
                    data: MapInstallError(None, id, error.to_string()),
                }))
            }
        }
    }

    async fn handle_request(config: DownloadQueueHandlerConfiguration, request: DownloadQueueRequest) {
        match request {
            DownloadQueueRequest::Map(map) => DownloadQueueHandler::download_map(config, map).await
        }
    }


    pub fn start(mut self) -> JoinHandle<()> {
        tokio::spawn(async move {
            let semaphore = Arc::new(Semaphore::new(self.config.config.concurrent_downloads as usize));
            loop {
                if let Some(request) = self.receiver.recv().await {
                    match semaphore.clone().acquire_owned().await {
                        Ok(permit) => {
                            let config = self.config.clone();
                            tokio::spawn(async move {
                                DownloadQueueHandler::handle_request(config, request).await;
                                drop(permit);
                            });
                        }
                        Err(err) => {
                            error!("Semaphore has been closed: {}", err);
                            panic!("Download queue exited");
                        }
                    }
                }
            }
        })
    }
}

pub struct InstallerQueueRequest {
    channel: tokio::sync::oneshot::Sender<InstallerQueueResult>,
    data: InstallerQueueData,
}

impl InstallerQueueRequest {
    pub fn create(channel: tokio::sync::oneshot::Sender<InstallerQueueResult>, data: InstallerQueueData) -> InstallerQueueRequest {
        InstallerQueueRequest {
            channel,
            data,
        }
    }
}

pub enum InstallerQueueData {
    Map(BeatSaverMap, MapVersion, Vec<u8>)
}

pub struct InstallerQueue {
    receiver: Receiver<InstallerQueueRequest>,
    config: LocalData,
    installer: Installer,
}

pub enum InstallerQueueResult {
    Success(BeatSaverMap, MapVersion),
    Error(BeatSaverMap, MapVersion, InstallerQueueError),
    AlreadyInstalled(BeatSaverMap, MapVersion)
}

#[derive(Error, Debug)]
pub enum InstallerQueueError {
    #[error("Failed to join async install task {0}")]
    JoinError(tokio::task::JoinError),
    #[error("Exceeded the maximum amount of retries")]
    TriesExceeded(),
}

impl InstallerQueue {
    pub fn new(receiver: Receiver<InstallerQueueRequest>, config: LocalData) -> InstallerQueue {
        InstallerQueue {
            receiver,
            installer: Installer::from(config.config.clone()),
            config,
        }
    }

    async fn install_map(&self, map: BeatSaverMap, version: MapVersion, data: Vec<u8>,
                         response: tokio::sync::oneshot::Sender<InstallerQueueResult>) {
        if self.config.map_index.lock().await
            .iter()
            .any(|map_data| map_data.has_hash(version.hash.as_str())) {
            response.send(InstallerQueueResult::AlreadyInstalled(map, version)).ok();
            return;
        }
        match self.installer.clone() {
            Installer::PC(pc) => {
                pc.install_map(map.clone(), data.as_ref());
                info!("PC install task succeeded!");
                if response.send(InstallerQueueResult::Success(map, version)).is_err() {
                    error!("Error when sending result");
                }
            }
            Installer::Quest(quest) => {
                let mut join_error = None;
                let mut success = false;
                for _ in 0..10 {
                    if let Some(handle) = quest.install_map(version.clone(), data.clone()) {
                        match handle.await {
                            Ok(result) => {
                                match result {
                                    Ok(_) => {
                                        info!("Quest install task succeeded!");
                                        success = true;
                                        break;
                                    }
                                    Err(err) => {
                                        error!("Task for quest installer failed: {}", err);
                                        error!("Backing off for 1 minute");
                                        tokio::time::sleep(std::time::Duration::from_secs(60)).await
                                    }
                                }
                            }
                            Err(err) => {
                                error!("Cannot join quest install task: {}", err);
                                join_error = Some(err);
                                break;
                            }
                        }
                    } else {
                        info!("Quest install task succeeded!");
                        success = true;
                        break;
                    }
                }
                if success {
                    if response.send(InstallerQueueResult::Success(map, version)).is_err() {
                        error!("Error when sending result");
                    }
                } else {
                    let error = if let Some(err) = join_error {
                        InstallerQueueError::JoinError(err)
                    } else {
                        InstallerQueueError::TriesExceeded()
                    };
                    if response.send(InstallerQueueResult::Error(map, version, error)).is_err() {
                        error!("Error when sending result");
                    }
                }
            }
        }
    }

    pub fn start(mut self) -> JoinHandle<()> {
        tokio::spawn(async move {
            loop {
                if let Some(request) = self.receiver.recv().await {
                    match request.data {
                        InstallerQueueData::Map(map, version, data) => self.install_map(map, version, data, request.channel).await
                    }
                }
            }
        })
    }
}