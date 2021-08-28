use thiserror::Error;
use std::time::Duration;

#[derive(Error, Debug)]
pub enum HttpError {
    #[error("An error occurred when trying to request {1}: {0}")]
    RequestError(reqwest::Error, String),
    #[error("Request to beatsaver returned error code: {0}")]
    StatusCodeError(u16),
    #[error("Failed to build request: {0}")]
    RequestBuildError(reqwest::Error),
}

pub async fn download<F: FnOnce(reqwest::Client) -> reqwest::RequestBuilder>(request_builder: F) -> Result<Vec<u8>, HttpError> {
    let client = construct_client();
    let request = request_builder(client.clone())
        .header("User-Agent", "AIOSaber-Client")
        .build()
        .map_err(|err| HttpError::RequestBuildError(err))?;
    let download_url = request.url().to_string();
    let result = client.execute(request).await;
    match result {
        Ok(response) => {
            if response.status().is_success() {
                match response.bytes().await {
                    Ok(bytes) => {
                        let buf = bytes.clone();
                        return Ok(buf.to_vec());
                    }
                    Err(error) => Err(HttpError::RequestError(error, download_url))
                }
            } else {
                Err(HttpError::StatusCodeError(response.status().as_u16()))
            }
        }
        Err(error) => Err(HttpError::RequestError(error, download_url))
    }
}

pub fn construct_client() -> reqwest::Client {
    reqwest::Client::builder()
        .connect_timeout(Duration::from_secs(5))
        .timeout(Duration::from_secs(600))
        .build().unwrap()
}