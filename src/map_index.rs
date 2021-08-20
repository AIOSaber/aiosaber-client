use thiserror::Error;
use std::path::PathBuf;
use std::io::Read;
use std::option::Option::Some;
use tokio::task::JoinError;
use futures_util::stream::StreamExt;
use log::debug;

#[derive(Error, Debug)]
pub enum IndexError {
    #[error("Cannot read the maps directory: {0}")]
    CannotReadMapsDir(std::io::Error),
    #[error("Cannot read that map: {0}")]
    CannotReadMap(std::io::Error),
    #[error("Directory is not a map {1}")]
    NotAMap(std::io::Error, PathBuf),
    #[error("Invalid map json {1}: {0}")]
    MapJsonError(serde_json::Error, PathBuf),
    #[error("Map info.dat has an invalid format: {0}")]
    InvalidMapInfoDat(PathBuf),
    #[error("Difficulty file cannot be read - {1}: {0}")]
    InvalidDifficulty(std::io::Error, PathBuf),
    #[error("An error occurred when trying to join another task - {1}: {0}")]
    JoinError(JoinError, PathBuf),
}

enum MaybeJoinHandle<T, H> {
    Handle(H),
    Raw(T),
}

pub async fn index_maps(path: PathBuf, aggressive: bool) -> Result<Vec<Result<(PathBuf, String), IndexError>>, IndexError> {
    let inner_path = path.clone();
    let read_result = tokio::task::spawn_blocking(move || {
        match std::fs::read_dir(inner_path) {
            Ok(dir) => {
                Ok(dir)
            }
            Err(err) => {
                Err(err)
            }
        }
    }).await;

    match read_result {
        Ok(result) => {
            match result {
                Ok(dir) => {
                    let mut vec = Vec::new();
                    for entry in dir {
                        match entry {
                            Ok(entry) => {
                                if aggressive {
                                    let handle = tokio::spawn(async move {
                                        process_map_directory(entry).await
                                    });
                                    vec.push(MaybeJoinHandle::Handle(handle));
                                } else {
                                    vec.push(MaybeJoinHandle::Raw(process_map_directory(entry).await))
                                }
                            }
                            Err(err) => {
                                vec.push(MaybeJoinHandle::Raw(Err(IndexError::CannotReadMap(err))))
                            }
                        }
                    }
                    let vec = futures_util::stream::iter(vec.into_iter())
                        .map(|handle| (handle, path.clone()))
                        .then(|(handle, path) | async move {
                            match handle {
                                MaybeJoinHandle::Handle(task) => {
                                    match task.await {
                                        Ok(result) => result,
                                        Err(err) => Err(IndexError::JoinError(err, path))
                                    }
                                }
                                MaybeJoinHandle::Raw(result) => result
                            }
                        })
                        .collect::<Vec<Result<(PathBuf, String), IndexError>>>().await;
                    Ok(vec)
                }
                Err(err) => {
                    Err(IndexError::CannotReadMapsDir(err))
                }
            }
        }
        Err(err) => {
            Err(IndexError::JoinError(err, path.clone()))
        }
    }
}

pub async fn process_map_directory(entry: std::fs::DirEntry) -> Result<(PathBuf, String), IndexError> {
    let buf = entry.path();
    debug!("Processing {}", buf.as_path().to_str().unwrap());
    let inner_buf = buf.clone();
    let async_hash = tokio::task::spawn_blocking(move || {
        match generate_hash(inner_buf.clone()) {
            Ok(hash) => {
                Ok((inner_buf, hash))
            }
            Err(err) => {
                Err(err)
            }
        }
    }).await;
    match async_hash {
        Ok(result) => {
            result
        }
        Err(err) => {
            Err(IndexError::JoinError(err, buf))
        }
    }
}

pub fn generate_hash(path: PathBuf) -> Result<String, IndexError> {
    let mut info_file_path = path.clone();
    info_file_path.push("info.dat");

    match std::fs::File::open(info_file_path.clone()) {
        Ok(mut info_file) => {
            let mut info_file_data = String::new();
            info_file.read_to_string(&mut info_file_data).ok();
            match serde_json::from_str(info_file_data.as_str()) {
                Ok(value) => {
                    let value: serde_json::Value = value;
                    let filenames = value.as_object()
                        .and_then(|obj| obj.get("_difficultyBeatmapSets"))
                        .and_then(|value| value.as_array())
                        .map(|vec| vec.iter()
                            .filter_map(|value| value.as_object())
                            .filter_map(|obj| obj.get("_difficultyBeatmaps"))
                            .filter_map(|value| value.as_array())
                            .flat_map(|array| array.iter()
                                .filter_map(|value| value.as_object())
                                .filter_map(|obj| obj.get("_beatmapFilename"))
                                .filter_map(|value| value.as_str())
                                .map(|str| str.to_string())
                                .collect::<Vec<String>>())
                            .collect::<Vec<String>>());
                    if let Some(filenames) = filenames {
                        let mut file_bufs = Vec::new();
                        for byte in info_file_data.as_bytes() {
                            file_bufs.push(byte.clone());
                        }
                        for filename in filenames {
                            let mut file_data_path = path.clone();
                            file_data_path.push(filename);
                            match std::fs::read(file_data_path.clone()) {
                                Ok(buf) => {
                                    for byte in buf {
                                        file_bufs.push(byte);
                                    }
                                }
                                Err(err) => {
                                    return Err(IndexError::InvalidDifficulty(err, file_data_path));
                                }
                            }
                        }
                        Ok(sha1::Sha1::from(file_bufs).hexdigest())
                    } else {
                        Err(IndexError::InvalidMapInfoDat(info_file_path))
                    }
                }
                Err(err) => {
                    Err(IndexError::MapJsonError(err, path))
                }
            }
        }
        Err(err) => {
            Err(IndexError::NotAMap(err, path))
        }
    }
}