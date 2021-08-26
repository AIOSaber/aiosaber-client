use crate::config::{LocalData, MapData, MapMetadata, AuditLogAction};
use log::{debug, info, warn, error};
use notify::{RecommendedWatcher, RecursiveMode, Watcher, DebouncedEvent};
use tokio::task::JoinHandle;
use std::path::PathBuf;
use crate::map_index;
use crate::beatsaver;

pub struct PcMapsWatcher {
    config: LocalData,
    path: PathBuf,
}

impl PcMapsWatcher {
    pub fn new(config: LocalData) -> PcMapsWatcher {
        let mut path = std::path::PathBuf::from(config.config.install_location.clone());
        path.push("Beat Saber_Data");
        path.push("CustomLevels");
        PcMapsWatcher {
            config,
            path,
        }
    }

    pub fn start_watcher(self) -> notify::Result<JoinHandle<()>> {
        info!("Starting watcher at {}", self.path.display());
        let (tx, rx) = std::sync::mpsc::channel();
        let mut watcher = notify::watcher(tx, core::time::Duration::from_secs(5))?;
        watcher.watch(self.path.as_path(), RecursiveMode::NonRecursive)?;
        Ok(tokio::spawn(async move { self.start_receiver(rx, watcher).await; }))
    }

    async fn handle_created(&self, config: LocalData, path: PathBuf) {
        match map_index::generate_hash(path.clone()) {
            Ok(hash) => {
                match beatsaver::resolve_map_by_hash(hash.as_str()).await {
                    Ok(map) => {
                        let mut mutex = config.map_index.lock().await;
                        mutex.push(MapData::Valid(MapMetadata {
                            path,
                            hash,
                            id: u32::from_str_radix(map.id.as_str(), 16).expect("Map id is not hex, wtf?"),
                        }));
                        config.audit_log_entry(AuditLogAction::MapInstall(map)).await;
                    }
                    Err(err) => {
                        warn!("Map seems to be not a beatsaver map {}: {:?}", path.display(), err);
                        let mut mutex = config.map_index.lock().await;
                        mutex.push(MapData::Unknown(path, hash));
                    }
                }
            }
            Err(err) => {
                warn!("watcher: An indexing error occurred in {}: {:?}", path.display(), err);
                let mut mutex = config.map_index.lock().await;
                mutex.push(MapData::Invalid(path));
            }
        }
        config.rewrite_map_index().await;
    }

    async fn handle_removed(&self, config: LocalData, path: PathBuf) {
        let mut mutex = config.map_index.lock().await;
        let meta: Option<MapMetadata> = mutex.iter()
            .filter_map(|map| map.into())
            .nth(0);
        mutex.retain(|entry| entry.as_ref().ne(&path));
        std::mem::drop(mutex);
        config.rewrite_map_index().await;
        if let Some(meta) = meta {
            config.audit_log_entry(AuditLogAction::MapDelete(meta.id, meta.hash)).await;
        }
    }

    async fn handle_renamed(&self, config: LocalData, old: PathBuf, new: PathBuf) {
        let mut mutex = config.map_index.lock().await;
        let old_data = mutex.iter()
            .find(|entry| entry.as_ref().eq(&old))
            .cloned();
        mutex.retain(|entry| entry.as_ref().ne(&old));
        let mut needs_rewrite = true;
        if let Some(old) = old_data {
            match old {
                MapData::Valid(meta) => {
                    mutex.push(MapData::Valid(MapMetadata {
                        path: new,
                        hash: meta.hash,
                        id: meta.id,
                    }));
                }
                MapData::Unknown(_, hash) => {
                    mutex.push(MapData::Unknown(new, hash));
                }
                MapData::Invalid(_) => {
                    mutex.push(MapData::Invalid(new));
                }
            }
            std::mem::drop(mutex);
        } else {
            std::mem::drop(mutex); // make sure its gone
            self.handle_created(config.clone(), new).await;
            needs_rewrite = false;
        }
        if needs_rewrite {
            config.rewrite_map_index().await;
        }
    }

    async fn start_receiver(self, channel: std::sync::mpsc::Receiver<notify::DebouncedEvent>, watcher: RecommendedWatcher) {
        let config = self.config.clone();
        let (tx, mut rx) = tokio::sync::mpsc::channel(128);
        let wrapper_handle = tokio::task::spawn_blocking(move || {
            loop {
                if let Ok(event) = channel.recv() {
                    debug!("std->tokio wrapper received event: {:?}", event);
                    let inner_tx = tx.clone();
                    tokio::spawn(async move {
                        inner_tx.send(event).await.ok();
                    });
                }
            }
        });

        let handle = tokio::spawn(async move {
            let watcher = std::mem::ManuallyDrop::new(watcher);
            let mut rcv_errs = 0u8;
            loop {
                if let Some(event) = rx.recv().await {
                    debug!("Received fs event: {:?}", event);
                    rcv_errs = 0;
                    match event {
                        DebouncedEvent::Create(path) => {
                            debug!("Created: {}", path.display());
                            self.handle_created(config.clone(), path).await;
                        }
                        DebouncedEvent::Remove(path) => {
                            debug!("Removed: {}", path.display());
                            self.handle_removed(config.clone(), path).await;
                        }
                        DebouncedEvent::Rename(old_path, new_path) => {
                            debug!("Renamed: {} -> {}", old_path.display(), new_path.display());
                            self.handle_renamed(config.clone(), old_path, new_path).await;
                        }
                        DebouncedEvent::Rescan => {
                            info!("Rescan required!");
                            config.clone().update_map_index(false).await;
                        }
                        DebouncedEvent::Error(error, path) => {
                            if let Some(path) = path {
                                error!("An error occurred at file {}: {}", path.display(), error);
                            } else {
                                error!("An error occurred: {}", error);
                            }
                        }
                        _ => {}
                    }
                } else {
                    error!("Receiver errored!");
                    rcv_errs += 1;
                    if rcv_errs >= 3 {
                        break;
                    }
                }
            }
            std::mem::drop(watcher);
        });
        tokio::select! {
            _val = wrapper_handle => {
                warn!("Wrapper handle died!");
            }
            _val = handle => {
                warn!("FSEvent handle died!");
            }
        }
    }
}