use crate::config::DaemonConfig;
use tokio::sync::mpsc::Receiver;
use tokio::task::JoinHandle;
use tokio::sync::Semaphore;
use log::{info, error};
use crate::beatsaver;
use crate::beatsaver::{MapVersion, BeatSaverMap};
use crate::websocket_handler::{WebSocketHandler, WebSocketMessage, ResultMsg, ConfigData};
use crate::websocket_handler::ResultMessageData::MapInstallError;
use crate::installer::Installer;
use std::sync::Arc;

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
                            if let Some(err) = installers.first().unwrap().installer_queue
                                .send(InstallerQueueRequest::Map(map, version, data))
                                .await
                                .err() {
                                error!("Failed to send map data to installer: {}", err);
                            }
                        } else {
                            for installer_data in installers {
                                if let Some(err) = installer_data.installer_queue
                                    .send(InstallerQueueRequest::Map(map.clone(), version.clone(), data.clone()))
                                    .await
                                    .err() {
                                    error!("Failed to send map data to installer: {}", err);
                                }
                            }
                        }
                    }
                    Err(error) => {
                        error!("BeatSaverDownloadError: {}", error);
                        WebSocketHandler::send_static(config.websocket.clone(), WebSocketMessage::ResultResponse(ResultMsg {
                            action: "InstallMaps".to_string(),
                            success: false,
                            data: MapInstallError(id, error.to_string()),
                        }))
                    }
                }
            }
            Err(error) => {
                error!("BeatSaverError: {}", error);
                WebSocketHandler::send_static(config.websocket.clone(), WebSocketMessage::ResultResponse(ResultMsg {
                    action: "InstallMaps".to_string(),
                    success: false,
                    data: MapInstallError(id, error.to_string()),
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

pub enum InstallerQueueRequest {
    Map(BeatSaverMap, MapVersion, Vec<u8>)
}

pub struct InstallerQueue {
    receiver: Receiver<InstallerQueueRequest>,
    installer: Installer,
}

impl InstallerQueue {
    pub fn new(receiver: Receiver<InstallerQueueRequest>, config: ConfigData) -> InstallerQueue {
        InstallerQueue {
            receiver,
            installer: Installer::from(config),
        }
    }

    async fn install_map(&self, map: BeatSaverMap, version: MapVersion, data: Vec<u8>) {
        match self.installer.clone() {
            Installer::PC(pc) => pc.install_map(map, data.as_ref()),
            Installer::Quest(quest) => {
                for _ in 0..10 {
                    if let Some(handle) = quest.install_map(version.clone(), data.clone()) {
                        match handle.await {
                            Ok(result) => {
                                match result {
                                    Ok(_) => {
                                        info!("Quest install task succeeded!");
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
                                break;
                            }
                        }
                    }
                }
            }
        }
    }

    pub fn start(mut self) -> JoinHandle<()> {
        tokio::spawn(async move {
            loop {
                if let Some(request) = self.receiver.recv().await {
                    match request {
                        InstallerQueueRequest::Map(map, version, data) => self.install_map(map, version, data).await
                    }
                }
            }
        })
    }
}