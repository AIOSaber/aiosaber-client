use std::time::Duration;
use serde::{Serialize, Deserialize};
use chrono::{DateTime, Utc};
use log::info;
use thiserror::Error;

#[derive(Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct BeatSaverMap {
    pub id: String,
    pub name: String,
    pub description: String,
    pub metadata: MapMetadata,
    pub automapper: bool,
    pub ranked: bool,
    pub qualified: bool,
    pub versions: Vec<MapVersion>,
}

#[derive(Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MapMetadata {
    pub bpm: f32,
    pub duration: u32,
    pub song_name: String,
    pub song_sub_name: String,
    pub song_author_name: String,
    pub level_author_name: String,
}

#[derive(Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MapVersion {
    pub hash: String,
    pub state: String,
    pub created_at: DateTime<Utc>,
    pub sage_score: u8,
    #[serde(alias = "downloadURL")] // thanks for a super odd name
    pub download_url: String,
}

#[derive(Error, Debug)]
pub enum BeatSaverError {
    #[error("An error occurred when trying to request {1}: {0}")]
    RequestError(reqwest::Error, String),
    #[error("Request to beatsaver returned error code: {0}")]
    StatusCodeError(u16),
    #[error("Error when deserializing json on: {1}: {0}\n{2}")]
    JsonError(serde_json::Error, String, String),
}

#[derive(Error, Debug)]
pub enum BeatSaverDownloadError {
    #[error(transparent)]
    BeatSaverError(#[from] BeatSaverError),
    #[error("Map {0} seems to have no versions")]
    NoMapVersion(String),
}

pub(crate) async fn retrieve_map_data(map: &BeatSaverMap) -> Result<(MapVersion, Vec<u8>), BeatSaverDownloadError> {
    if let Some(version) = find_latest_version(map) {
        info!("Downloading map with hash {}", version.hash.as_str());
        download_zip(&version).await
            .map(|data| (version, data))
            .map_err(|err| err.into())
    } else {
        Err(BeatSaverDownloadError::NoMapVersion(map.id.clone()))
    }
}

pub fn find_latest_version(map: &BeatSaverMap) -> Option<MapVersion> {
    let mut versions = map.versions.clone();
    versions.sort_by_key(|map| map.created_at);
    versions.pop()
}

pub async fn download_zip(version: &MapVersion) -> Result<Vec<u8>, BeatSaverError> {
    let client = reqwest::Client::builder()
        .connect_timeout(Duration::from_secs(5))
        .timeout(Duration::from_secs(30))
        .build().unwrap();
    let download_url = version.download_url.clone();
    let result = client.get(download_url.clone())
        .header("User-Agent", "AIOSaber-Client")
        .send()
        .await;
    match result {
        Ok(response) => {
            if response.status().is_success() {
                match response.bytes().await {
                    Ok(bytes) => {
                        let buf = bytes.clone();
                        return Ok(buf.to_vec());
                    }
                    Err(error) => Err(BeatSaverError::RequestError(error, download_url))
                }
            } else {
                Err(BeatSaverError::StatusCodeError(response.status().as_u16()))
            }
        }
        Err(error) => Err(BeatSaverError::RequestError(error, download_url))
    }
}

pub async fn resolve_map_by_id(id: String) -> Result<BeatSaverMap, BeatSaverError> {
    let mut url = "https://beatsaver.com/api/maps/id/".to_string();
    url.push_str(id.as_str());
    execute_beatsaver_map_request(url).await
}

pub async fn resolve_map_by_hash(hash: String) -> Result<BeatSaverMap, BeatSaverError> {
    let mut url = "https://beatsaver.com/api/maps/hash/".to_string();
    url.push_str(hash.as_str());
    execute_beatsaver_map_request(url).await
}

async fn execute_beatsaver_map_request(url: String) -> Result<BeatSaverMap, BeatSaverError> {
    let client = reqwest::Client::builder()
        .connect_timeout(Duration::from_secs(5))
        .timeout(Duration::from_secs(5))
        .build()
        .unwrap();
    match client.get(url.clone())
        .header("User-Agent", "AIOSaber-Client")
        .send().await {
        Ok(response) => {
            if response.status().is_success() {
                match response.text().await {
                    Ok(json) => {
                        match serde_json::from_str(json.as_str()) {
                            Ok(map) => Ok(map),
                            Err(err) => {
                                Err(BeatSaverError::JsonError(err, url, json))
                            }
                        }
                    }
                    Err(err) => Err(BeatSaverError::RequestError(err, url))
                }
            } else {
                Err(BeatSaverError::StatusCodeError(response.status().as_u16()))
            }
        }
        Err(err) => Err(BeatSaverError::RequestError(err, url))
    }
}