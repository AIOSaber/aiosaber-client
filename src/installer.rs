use crate::websocket_handler::{ConfigData, InstallType};
use std::time::Duration;
use log::{debug, info, error};
use std::io::{Cursor, Read, Seek};
use zip::ZipArchive;
use zip::result::ZipError;
use std::{fs, io};
use std::path::PathBuf;
use crate::installer::Installer::{PC, Quest};

pub enum Installer {
    PC(PcInstaller),
    Quest(QuestInstaller),
}

pub struct PcInstaller {
    config: ConfigData
}

pub struct QuestInstaller {
    config: ConfigData
}

impl From<ConfigData> for Installer {
    fn from(config: ConfigData) -> Self {
        match config.install_type {
            InstallType::PC => PC(PcInstaller { config }),
            InstallType::Quest => Quest(QuestInstaller { config })
        }
    }
}

impl PcInstaller {

    pub async fn install_map(&self, folder_name: String, download_url: String) {
        let client = reqwest::Client::builder()
            .connect_timeout(Duration::from_secs(5))
            .timeout(Duration::from_secs(30))
            .build().unwrap();
        info!("Downloading map from {}", download_url);
        let result = client.get(download_url)
            .header("User-Agent", "AIOSaber-Client")
            .send()
            .await;
        match result {
            Ok(response) => {
                match response.bytes().await {
                    Ok(bytes) => {
                        let target = self.config.install_location.clone();
                        let mut target = PathBuf::from(target);
                        target.push("Beat Saber_Data");
                        target.push("CustomLevels");
                        target.push(folder_name
                            .replace("\\", "")
                            .replace("/", "")
                            .replace("*", "")
                            .replace("?", "")
                            .replace("\"", "")
                            .replace("<", "")
                            .replace(">", "")
                            .replace("|", ""));
                        info!("Unzipping to {}", target.display());
                        let buf = bytes.as_ref();
                        let buf = Cursor::new(buf);
                        let archive = zip::ZipArchive::new(buf);
                        match archive {
                            Ok(archive) => {
                                unzip_to(archive, target);
                                info!("Done!");
                            }
                            Err(error) => {
                                match error {
                                    ZipError::Io(error) => error!("IOError when reading zip: {}", error),
                                    ZipError::InvalidArchive(error) => error!("Invalid zip archive: {}", error),
                                    ZipError::UnsupportedArchive(error) => error!("Unsupported archive format: {}", error),
                                    ZipError::FileNotFound => error!("File not found")
                                }
                            }
                        }
                    }
                    Err(error) => error!("An error occurred: {}", error)
                }
            }
            Err(error) => error!("An error occurred: {}", error)
        }
    }
}

fn unzip_to<R: Read + Seek>(mut archive: ZipArchive<R>, target: PathBuf) {
    fs::create_dir_all(&target).ok();
    for i in 0..archive.len() {
        let mut file = archive.by_index(i).unwrap();
        let outpath = match file.enclosed_name() {
            Some(path) => path.to_owned(),
            None => continue,
        };
        let mut full_path = target.clone();
        full_path.push(outpath);
        let outpath = full_path;

        {
            let comment = file.comment();
            if !comment.is_empty() {
                debug!("File {} comment: {}", i, comment);
            }
        }

        if (&*file.name()).ends_with('/') {
            debug!("File {} extracted to \"{}\"", i, outpath.display());
            fs::create_dir_all(&outpath).unwrap();
        } else {
            debug!(
                "File {} extracted to \"{}\" ({} bytes)",
                i,
                outpath.display(),
                file.size()
            );
            if let Some(p) = outpath.parent() {
                if !p.exists() {
                    fs::create_dir_all(&p).unwrap();
                }
            }
            let mut outfile = fs::File::create(&outpath).unwrap();
            io::copy(&mut file, &mut outfile).unwrap();
        }
    }
}