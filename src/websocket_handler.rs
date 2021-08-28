use tokio::task::JoinHandle;
use warp::ws::Message;
use log::{info, error};
use serde::{Deserialize, Serialize};
use std::str::FromStr;
use crate::config::DaemonConfig;
use uuid::Uuid;

pub struct WebSocketHandler {
    rx: tokio::sync::mpsc::Receiver<Message>,
    tx: tokio::sync::broadcast::Sender<Message>,
    config: DaemonConfig,
}

#[derive(Clone, Deserialize, Serialize)]
#[serde(tag = "type", content = "data")]
pub enum WebSocketMessage {
    Connected(Vec<ConfigData>),
    UpdateConfig(Vec<ConfigData>),
    SetupOneClick(),
    ResultResponse(ResultMsg),
    InstallMaps(Vec<String>),
    InstallMods(Vec<WebSocketMod>),
}

#[derive(Clone, Deserialize, Serialize)]
#[serde(tag = "type", content = "data")]
pub enum WebSocketMod {
    PcMod(WebSocketPcModData),
    QuestMod(WebSocketQuestModData)
}

#[derive(Clone, Deserialize, Serialize)]
pub struct WebSocketCommonModData {
    pub identifier: String,
    pub url: String,
}

#[derive(Clone, Deserialize, Serialize)]
pub struct WebSocketQuestModData {
    #[serde(flatten)]
    pub common: WebSocketCommonModData
}

#[derive(Clone, Deserialize, Serialize)]
pub struct WebSocketPcModData {
    #[serde(flatten)]
    pub common: WebSocketCommonModData,
    #[serde(flatten)]
    pub data: WebSocketPcModType
}

#[derive(Clone, Deserialize, Serialize)]
#[serde(tag = "mod_type", content = "content")]
pub enum WebSocketPcModType {
    DLL(String),
    ZIP(Option<String>)
}

#[derive(Clone, Deserialize, Serialize)]
pub struct ConfigData {
    pub id: Uuid,
    pub rest_token: String,
    pub install_type: InstallType,
    pub install_location: String,
}

#[derive(Clone, Deserialize, Serialize, PartialEq)]
pub enum InstallType {
    PC,
    Quest,
}

#[derive(Clone, Deserialize, Serialize)]
pub struct ResultMsg {
    pub(crate) action: String,
    pub(crate) success: bool,
    pub(crate) data: ResultMessageData,
}

#[derive(Clone, Deserialize, Serialize)]
#[serde(untagged)]
pub enum ResultMessageData {
    Simple(String),
    MapInstallError(Option<Uuid>, String, String),
    MapInstallSuccess(Uuid, String, String),
    ModInstallError(String, String)
}

impl ToString for InstallType {
    fn to_string(&self) -> String {
        match self {
            InstallType::PC => "PC".to_owned(),
            InstallType::Quest => "Quest".to_owned()
        }
    }
}

impl FromStr for InstallType {
    type Err = ();

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            "PC" => Ok(InstallType::PC),
            "Quest" => Ok(InstallType::Quest),
            _ => Err(())
        }
    }
}

#[derive(Clone, Deserialize, Serialize)]
pub struct InstallData {
    location: String,
    data: Vec<String>,
}

#[derive(Clone, Deserialize, Serialize)]
pub struct ClientIdentity {
    pub unique_id: String,
}

impl WebSocketHandler {
    pub fn new(tx: tokio::sync::broadcast::Sender<Message>,
               rx: tokio::sync::mpsc::Receiver<Message>,
               config: DaemonConfig) -> WebSocketHandler {
        WebSocketHandler {
            rx,
            tx,
            config,
        }
    }

    pub fn get_sender(&self) -> tokio::sync::broadcast::Sender<Message> {
        self.tx.clone()
    }

    pub(crate) fn start(mut self) -> JoinHandle<()> {
        tokio::spawn(async move {
            let tx = self.tx.clone();
            while let Some(message) = self.rx.recv().await {
                if message.is_ping() {
                    tx.send(Message::pong(message.into_bytes())).ok();
                } else if message.is_text() {
                    if let Ok(text) = message.to_str() {
                        let msg: WebSocketMessage = match serde_json::from_str(text) {
                            Ok(parsed) => parsed,
                            Err(error) => {
                                error!("An error occurred when trying to parse incoming WebSocket message: {}", error);
                                continue;
                            }
                        };
                        if let Some(msg) = self.handle_web_socket_message(msg).await {
                            tx.send(Message::text(serde_json::to_string(&msg).unwrap())).ok();
                        }
                    }
                }
            }
        })
    }

    pub async fn handle_web_socket_message(&self, message: WebSocketMessage) -> Option<WebSocketMessage> {
        let action = message.to_string();
        match message {
            WebSocketMessage::UpdateConfig(configs) => {
                info!("Updating configs...");
                let updated = self.config.update_configs(configs).await;
                Some(WebSocketMessage::Connected(updated))
            }
            WebSocketMessage::SetupOneClick() => {
                info!("Setting up one-click...");
                let result = crate::one_click::register_one_click();
                let success = result.is_ok();
                let msg = match result {
                    Ok(msg) => msg,
                    Err(msg) => msg
                };
                Some(WebSocketMessage::ResultResponse(ResultMsg {
                    action,
                    success,
                    data: ResultMessageData::Simple(msg),
                }))
            }
            WebSocketMessage::InstallMaps(maps) => {
                for map in maps.into_iter() {
                    self.config.queue_map(map).await.ok();
                }
                None
            }
            WebSocketMessage::InstallMods(mods) => {
                for mod_data in mods.into_iter() {
                    self.config.queue_mod(mod_data).await.ok();
                }
                None
            }
            _ => {
                error!("Received client message from server");
                None
            }
        }
    }

    pub fn send_static(tx: tokio::sync::broadcast::Sender<Message>, message: WebSocketMessage) {
        tx.send(Message::text(serde_json::to_string(&message).unwrap())).ok();
    }
}

impl ToString for WebSocketMessage {
    fn to_string(&self) -> String {
        match self {
            WebSocketMessage::Connected(_) => "Connected",
            WebSocketMessage::UpdateConfig(_) => "UpdateConfig",
            WebSocketMessage::SetupOneClick() => "SetupOneClick",
            WebSocketMessage::ResultResponse(_) => "ResultResponse",
            WebSocketMessage::InstallMaps(_) => "InstallMaps",
            WebSocketMessage::InstallMods(_) => "InstallMods",
        }.to_string()
    }
}