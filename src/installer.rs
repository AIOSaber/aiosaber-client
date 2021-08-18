use crate::websocket_handler::{ConfigData, InstallType};
use std::time::Duration;
use log::{debug, info, warn, error};
use std::io::{Cursor, Read, Seek};
use zip::ZipArchive;
use zip::result::ZipError;
use std::{fs, io, env};
use std::path::PathBuf;
use crate::installer::Installer::{PC, Quest};
use crate::beatsaver::{BeatSaverMap, MapVersion};
use tokio::task::JoinHandle;
use curl::easy::{Form, List};
use std::process::Command;

#[derive(Clone)]
pub enum Installer {
    PC(PcInstaller),
    Quest(QuestInstaller),
}

#[derive(Clone)]
pub struct PcInstaller {
    config: ConfigData
}

#[derive(Clone)]
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
    pub fn install_map(&self, map: BeatSaverMap, data: &[u8]) {
        let mut full_name = map.id.clone();
        full_name.push_str(" (");
        full_name.push_str(map.metadata.song_name.as_str());
        full_name.push_str(" - ");
        full_name.push_str(map.metadata.level_author_name.as_str());
        full_name.push_str(")");

        let target = self.config.install_location.clone();
        let mut target = PathBuf::from(target);
        target.push("Beat Saber_Data");
        target.push("CustomLevels");
        target.push(full_name
            .replace("\\", "")
            .replace("/", "")
            .replace("*", "")
            .replace("?", "")
            .replace("\"", "")
            .replace("<", "")
            .replace(">", "")
            .replace("|", ""));
        info!("Unzipping to {}", target.display());
        if let Ok(archive) = as_zip_archive(data) {
            unzip_to(archive, target);
        }
    }
}

impl QuestInstaller {
    pub fn install_map(&self, version: MapVersion, data: Vec<u8>) -> Option<JoinHandle<Result<(), String>>> {
        let full_name = version.clone().hash;

        if self.config.install_location.starts_with("adb://") {
            let mut unpack_dir = env::current_dir().unwrap();
            unpack_dir.push("unpack");

            let mut tmp_dir = unpack_dir.clone();
            tmp_dir.push(full_name.clone());

            if let Ok(archive) = as_zip_archive(data.as_ref()) {
                unzip_to(archive, tmp_dir.clone());
            }

            let adb_target = &self.config.install_location[6..];
            if adb_target.eq("usb") {
                info!("Using ADB via USB");
            } else {
                info!("Trying to use ADB @ {}", adb_target);
                let mut connect_cmd = "adb connect ".to_owned();
                connect_cmd.push_str(adb_target);
                match execute_adb(connect_cmd) {
                    Ok(_) => info!("adb: Connected via network"),
                    Err(err) => {
                        if let Some(err) = err {
                            error!("Couldn't start adb (is it installed / in path?): {}", err);
                        } else {
                            error!("adb: Couldn't connect to device");
                        }
                    }
                }
            }
            let mut dst_folder = "/sdcard/ModData/com.beatgames.beatsaber/Mods/SongLoader/CustomLevels/".to_owned();
            dst_folder.push_str(full_name.as_str());
            dst_folder.push_str("/");
            let mut command = "adb shell mkdir -p ".to_owned();
            command.push_str(dst_folder.as_str());
            match execute_adb(command) {
                Ok(_) => info!("adb: Created folder"),
                Err(err) => {
                    if let Some(err) = err {
                        error!("Couldn't start adb (is it installed / in path?): {}", err);
                    } else {
                        error!("adb: Couldn't create folder");
                    }
                }
            }
            let mut push_cmd = "adb push ".to_owned();
            push_cmd.push_str(tmp_dir.to_str().unwrap());
            push_cmd.push_str("/. ");
            push_cmd.push_str(dst_folder.as_str());
            match execute_adb(push_cmd) {
                Ok(_) => info!("adb: Copied files"),
                Err(err) => {
                    if let Some(err) = err {
                        error!("Couldn't start adb (is it installed / in path?): {}", err);
                    } else {
                        error!("adb: Couldn't copy files");
                    }
                }
            }
            None
        } else {
            // bmbf upload
            let mut bmbf_host = self.config.install_location.clone();
            info!("Uploading map to BMBF @ {}", bmbf_host.as_str());
            bmbf_host.push_str("/host/beatsaber/upload");
            let mut bmbf_referer = self.config.install_location.clone();
            bmbf_referer.push_str("/main/upload");
            let mut referer_header = "Referer: ".to_owned();
            referer_header.push_str(bmbf_referer.as_str());
            let data = data.clone();
            Some(tokio::spawn(async move {
                let mut curl = curl::easy::Easy::new();
                curl.url(bmbf_host.as_str()).unwrap();
                curl.post(true).unwrap();
                let mut headers = List::new();
                headers.append(referer_header.as_str()).unwrap();
                headers.append("Connection: keep-alive").unwrap();
                curl.http_headers(headers).unwrap();
                let mut form = Form::new();
                form.part("file")
                    .buffer(version.hash.as_str(), data)
                    .add()
                    .unwrap();
                curl.httppost(form).unwrap();
                curl.verbose(true).unwrap();
                match curl.perform() {
                    Ok(_) => {
                        let response_code = curl.response_code().unwrap_or(0);
                        if response_code == 204 {
                            info!("Done!");
                            Ok(())
                        } else {
                            error!("Invalid response: {}", response_code);
                            Err("Invalid response code".to_owned())
                        }
                    }
                    Err(err) => {
                        error!("An error occurred when sending request: {}", err);
                        Err("Request error".to_owned())
                    }
                }
            }))
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

pub(crate) async fn retrieve_map_data(map: &BeatSaverMap) -> Result<(MapVersion, Vec<u8>), ()> {
    let client = reqwest::Client::builder()
        .connect_timeout(Duration::from_secs(5))
        .timeout(Duration::from_secs(30))
        .build().unwrap();

    // Find latest version
    let mut version = None;
    for map_version in &map.versions {
        if version.is_none() {
            version = Some(map_version.clone());
            continue;
        }
        if version.clone().unwrap().created_at.gt(&map_version.created_at) {
            continue;
        }
        version = Some(map_version.clone());
    }

    if version.is_none() {
        error!("No version found");
        return Err(());
    }

    let version = version.unwrap();

    info!("Downloading map with hash {}", version.hash.as_str());
    let result = client.get(version.download_url.clone())
        .header("User-Agent", "AIOSaber-Client")
        .send()
        .await;
    match result {
        Ok(response) => {
            match response.bytes().await {
                Ok(bytes) => {
                    let buf = bytes.clone();
                    return Ok((version, buf.to_vec()));
                }
                Err(error) => error!("An error occurred: {}", error)
            }
        }
        Err(error) => error!("An error occurred: {}", error)
    }
    Err(())
}

fn as_zip_archive(bytes: &[u8]) -> Result<ZipArchive<Cursor<&[u8]>>, ()> {
    let buf = Cursor::new(bytes);
    let archive = zip::ZipArchive::new(buf);
    match archive {
        Ok(archive) => {
            return Ok(archive);
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
    Err(())
}

fn execute_adb(command: String) -> Result<(), Option<std::io::Error>> {
    let mut cmd = Command::new(command);
    if let Some(path) = env::var("PATH").ok() {
        cmd.env("PATH", path);
    } else {
        warn!("No path variable found to forward to subprocess");
    }
    match cmd.spawn() {
        Ok(mut process) => {
            if process.wait()
                .map(|status| status.success())
                .unwrap_or(false) {
                Ok(())
            } else {
                Err(None)
            }
        }
        Err(err) => {
            Err(Some(err))
        }
    }
}