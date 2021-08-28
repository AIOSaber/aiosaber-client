use std::time::Duration;
use serde::{Serialize, Deserialize};
use chrono::{DateTime, Utc};
use log::info;
use thiserror::Error;
use crate::http_client::HttpError;

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
    #[serde(alias = "downloadURL")] // thanks for a super odd name
    pub download_url: String,
}

#[derive(Error, Debug)]
pub enum BeatSaverError {
    #[error(transparent)]
    HttpError(HttpError),
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
    crate::http_client::download(move |client| client.get(version.download_url.clone()))
        .await
        .map_err(|err| BeatSaverError::HttpError(err))
}

pub fn get_beatsaver_base_url() -> String {
    let mut api_url = option_env!("BEATSAVER_API_URL")
        .unwrap_or("https://beatsaver.com/api/")
        .to_owned();
    if !api_url.ends_with("/") {
        api_url.push_str("/");
    }
    api_url
}

pub async fn resolve_map_by_id(id: &str) -> Result<BeatSaverMap, BeatSaverError> {
    let mut url = get_beatsaver_base_url();
    url.push_str("maps/id/");
    url.push_str(id);
    execute_beatsaver_map_request(url).await
}

pub async fn resolve_map_by_hash(hash: &str) -> Result<BeatSaverMap, BeatSaverError> {
    let mut url = get_beatsaver_base_url();
    url.push_str("maps/id/");
    url.push_str(hash);
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