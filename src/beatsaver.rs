use serde_json::Value;
use std::time::Duration;

pub async fn resolve_download_url(hash: String) -> Result<(String, String), String> {
    let client = reqwest::Client::builder()
        .connect_timeout(Duration::from_secs(5))
        .timeout(Duration::from_secs(5))
        .build()
        .unwrap();
    let mut url = "https://beatsaver.com/api/maps/id/".to_string();
    url.push_str(hash.as_str());
    let mut referrer = "https://beatsaver.com/maps/".to_string();
    referrer.push_str(hash.as_str());
    match client.get(url)
        .header("User-Agent", "AIOSaber-Client")
        .send().await {
        Ok(response) => {
            match response.text().await {
                Ok(json) => {
                    if let Ok(node) = serde_json::from_str(json.as_str()) {
                        let node: Value = node;
                        let download_url: Option<String> = node.as_object()
                            .and_then(|obj| obj.get("versions"))
                            .and_then(|value| value.as_array())
                            .and_then(|array| array.last())
                            .and_then(|value| value.as_object())
                            .and_then(|obj| obj.get("downloadURL"))
                            .and_then(|value| value.as_str())
                            .map(|str| str.to_owned());
                        let name: String = node.as_object()
                            .and_then(|obj| obj.get("name"))
                            .and_then(|value| value.as_str())
                            .map(|str| str.to_owned())
                            .unwrap_or("No Name".to_string());
                        if let Some(download_url) = download_url {
                            let mut full_name = hash.clone();
                            full_name.push_str(" (");
                            full_name.push_str(name.as_str());
                            full_name.push_str(")");
                            Ok((download_url, full_name))
                        } else {
                            Err("No download URL found".to_string())
                        }
                    } else {
                        Err("Error unwrapping json".to_string())
                    }
                }
                Err(error) => Err(error.to_string())
            }
        }
        Err(err) => Err(err.status().map(|sc| sc.as_str().to_string()).unwrap_or("Not connected".to_string()))
    }
}