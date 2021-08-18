use std::time::Duration;
use serde::{Serialize, Deserialize};
use chrono::{DateTime, Utc};
use log::error;

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
    #[serde(alias="downloadURL")] // thanks for a super odd name
    pub download_url: String,
}

pub async fn resolve_map_by_id(id: String) -> Result<BeatSaverMap, String> {
    let client = reqwest::Client::builder()
        .connect_timeout(Duration::from_secs(5))
        .timeout(Duration::from_secs(5))
        .build()
        .unwrap();
    let mut url = "https://beatsaver.com/api/maps/id/".to_string();
    url.push_str(id.as_str());
    match client.get(url)
        .header("User-Agent", "AIOSaber-Client")
        .send().await {
        Ok(response) => {
            match response.text().await {
                Ok(json) => {
                    match serde_json::from_str(json.as_str()) {
                        Ok(map) => Ok(map),
                        Err(err) => {
                            error!("JSON Error: {}", err);
                            Err("Error unwrapping json".to_string())
                        }
                    }
                }
                Err(error) => Err(error.to_string())
            }
        }
        Err(err) => Err(err.status().map(|sc| sc.as_str().to_string()).unwrap_or("Not connected".to_string()))
    }
}