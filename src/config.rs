use crate::websocket_handler::{ConfigData, InstallType};
use std::sync::Arc;
use tokio::sync::Mutex;
use std::fs::File;
use std::io::{Read, Write};
use yaml_rust::{YamlLoader, Yaml, YamlEmitter};
use log::{info, warn, error};
use std::str::FromStr;
use crate::installer::Installer;
use std::env;

#[derive(Clone)]
pub struct DaemonConfig {
    current_configs: Arc<Mutex<Vec<ConfigData>>>
}

impl DaemonConfig {
    pub fn new() -> DaemonConfig {
        DaemonConfig {
            current_configs: Arc::new(Mutex::new(DaemonConfig::read_from_file()))
        }
    }

    fn read_from_file() -> Vec<ConfigData> {
        let mut path = env::current_dir().unwrap().clone();
        path.push("daemon-config.yaml");
        let mut vec = Vec::new();
        match File::open(path.clone()) {
            Ok(mut file) => {
                let mut contents = String::new();
                file.read_to_string(&mut contents).ok();
                match YamlLoader::load_from_str(contents.as_str()) {
                    Ok(docs) => {
                        for yaml in docs {
                            if let Ok(config) = DaemonConfig::read_yaml_doc(yaml) {
                                vec.push(config);
                            }
                        }
                    }
                    Err(error) => {
                        warn!("Invalid yaml configuration: {}", error);
                    }
                }
            }
            Err(err) => {
                warn!("Couldn't open configuration file {}: {}", path.display(), err);
            }
        }
        vec
    }

    fn read_yaml_doc(yaml: Yaml) -> Result<ConfigData, ()> {
        if let Some(map) = yaml.as_hash() {
            let rest_token = map.get(&Yaml::String("restToken".to_string()))
                .and_then(|yaml| yaml.as_str())
                .map(|str| str.to_string());
            let install_type = map.get(&Yaml::String("installType".to_string()))
                .and_then(|yaml| yaml.as_str())
                .and_then(|str| InstallType::from_str(str).ok());
            let install_location = map.get(&Yaml::String("installLocation".to_string()))
                .and_then(|yaml| yaml.as_str())
                .map(|str| str.to_string());
            if let Some(((rest_token, install_type), install_location)) = rest_token
                .zip(install_type)
                .zip(install_location) {
                return Ok(ConfigData {
                    rest_token,
                    install_type,
                    install_location,
                });
            }
        }
        Err(())
    }

    fn write_to_file(configs: Vec<ConfigData>) {
        info!("Writing changed config to file...");
        let mut out_str = String::new();
        let mut emitter = YamlEmitter::new(&mut out_str);
        for config_data in configs {
            let mut hash = yaml_rust::yaml::Hash::new();
            hash.insert(Yaml::String("restToken".to_owned()), Yaml::String(config_data.rest_token.clone()));
            hash.insert(Yaml::String("installType".to_owned()), Yaml::String(config_data.install_type.to_string()));
            hash.insert(Yaml::String("installLocation".to_owned()), Yaml::String(config_data.install_location.clone()));
            let yaml = Yaml::Hash(hash);
            emitter.dump(&yaml).ok();
        }

        let mut path = env::current_dir().unwrap().clone();
        path.push("daemon-config.yaml");
        if let Ok(mut file) = File::create(path) {
            file.write_all(out_str.as_bytes()).ok();
            info!("Done");
        } else {
            error!("An error occurred when writing file to system");
        }
    }

    pub async fn replace_configs(&self, configs: Vec<ConfigData>) -> Vec<ConfigData> {
        let mut mutex = self.current_configs.lock().await;
        mutex.clear();
        for config_data in configs {
            mutex.push(config_data);
        }
        std::mem::drop(mutex);
        let configs = self.get_configs().await;
        DaemonConfig::write_to_file(configs.clone());
        configs
    }

    pub async fn get_configs(&self) -> Vec<ConfigData> {
        let mutex = self.current_configs.lock().await;
        let mut vec = Vec::new();
        for config in mutex.iter() {
            vec.push(config.clone());
        }
        vec
    }
}

impl Into<Vec<Installer>> for DaemonConfig {
    fn into(self) -> Vec<Installer> {
        let mut vec = Vec::new();
        for config in self.current_configs.try_lock().unwrap().iter() {
            vec.push(Installer::from(config.clone()));
        }
        vec
    }
}
