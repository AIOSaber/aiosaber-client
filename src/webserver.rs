use tokio::task::JoinHandle;
use std::net::SocketAddr;
use warp::Filter;
use std::process::exit;
use log::{trace, debug, info, warn, error};
use tokio::time::Duration;
use futures_util::{StreamExt, SinkExt, TryFutureExt};
use crate::websocket_handler::{WebSocketHandler, WebSocketMessage};
use crate::config::DaemonConfig;
use warp::http::StatusCode;

pub struct WebServer {
    version: String,
    config: DaemonConfig,
}

impl WebServer {
    pub(crate) fn create_server(version: String, config: DaemonConfig) -> WebServer {
        WebServer {
            version,
            config,
        }
    }

    pub(crate) fn start(self, addr: SocketAddr) -> (JoinHandle<()>, WebSocketHandler) {
        let (ws_outbound_tx, _) = tokio::sync::broadcast::channel(32);
        let (ws_inbound_tx, ws_inbound_rx) = tokio::sync::mpsc::channel(32);

        let handler = WebSocketHandler::new(ws_outbound_tx.clone(), ws_inbound_rx, self.config.clone());
        let config = self.config.clone();
        let web_server = tokio::spawn(async move {
            let cors = warp::cors()
                .allow_methods(vec!["GET", "POST"])
                .allow_origins(vec!["https://beatsaver.com", "http://beatsaver.com", "https://scoresaber.com"]);

            let shutdown = warp::get()
                .and(warp::path("shutdown"))
                .and_then(WebServer::shutdown);

            let options = warp::options().map(WebServer::options);

            let queue_config = config.clone();
            let queue_map = warp::path!("queue" / "map" / String)
                .and(warp::post())
                .and(warp::any().map(move || queue_config.clone()))
                .and_then(|id, config| async move {
                    WebServer::queue_install(config, id).await
                }).with(cors.clone());

            let version = self.version.clone();
            let version_info = warp::path!("version")
                .and(warp::get())
                .and(warp::any().map(move || version.clone()))
                .map(|version| Ok(Box::new(version)))
                .with(cors);

            let ws_tx = ws_outbound_tx.clone();
            let ws_inbound_tx = ws_inbound_tx.clone();
            let config = config.clone();
            let websocket = warp::path("pipe")
                .and(warp::ws())
                .and(warp::any().map(move || ws_tx.clone()))
                .and(warp::any().map(move || ws_inbound_tx.clone()))
                .and(warp::any().map(move || config.clone()))
                .map(|ws: warp::ws::Ws, tx, inbound_tx, config| {
                    trace!("WebSocket connection created!");
                    ws.on_upgrade(move |websocket| WebServer::websocket_connected(websocket, tx, inbound_tx, config))
                });

            warp::serve(
                options
                    .or(version_info)
                    .or(queue_map)
                    .or(websocket)
                    .or(shutdown),
            )
                .run(addr)
                .await;
        });
        (web_server, handler)
    }

    fn options() -> Box<dyn warp::Reply> {
        Box::new("OK")
    }

    async fn shutdown() -> Result<Box<dyn warp::Reply>, warp::Rejection> {
        tokio::spawn(async move {
            tokio::time::sleep(Duration::from_secs(1)).await;
            exit(0);
        });
        Ok(Box::new("OK"))
    }

    async fn queue_install(config: DaemonConfig, id: String) -> Result<Box<dyn warp::Reply>, warp::Rejection> {
        match config.queue_map(id).await {
            Ok(_) => Ok(Box::new(warp::reply::with_status("", StatusCode::NO_CONTENT))),
            Err(err) => {
                error!("An error occurred when trying to submit map into download queue: {}", err);
                Ok(Box::new(warp::reply::with_status("", StatusCode::INTERNAL_SERVER_ERROR)))
            }
        }
    }

    async fn websocket_connected(websocket: warp::ws::WebSocket,
                                 tx: tokio::sync::broadcast::Sender<warp::ws::Message>,
                                 inbound_tx: tokio::sync::mpsc::Sender<warp::ws::Message>,
                                 config: DaemonConfig) {
        info!("WebSocket connection upgrade (connected)!");
        let (mut ws_tx, mut ws_rx) = websocket.split();

        let sender_task_tx = tx.clone();
        let handle = tokio::spawn(async move {
            while let Ok(message) = sender_task_tx.subscribe().recv().await {
                if message.is_text() {
                    trace!("Sending message: {}", message.to_str().unwrap());
                }
                ws_tx
                    .send(message)
                    .unwrap_or_else(|e| {
                        error!("Error when sending to WS client: {}", e)
                    }).await;
            }
        });

        tokio::spawn(async move {
            tokio::time::sleep(Duration::from_millis(250)).await;
            WebSocketHandler::send_static(tx, WebSocketMessage::Connected(config.get_configs().await));
        });

        while let Some(result) = ws_rx.next().await {
            match result {
                Ok(msg) => {
                    if msg.is_text() {
                        debug!("Received text message: {}", msg.to_str().unwrap());
                    }
                    if msg.is_ping() {
                        trace!("Received ping message");
                    }
                    if msg.is_pong() {
                        trace!("Received pong message");
                    }
                    if msg.is_close() {
                        info!("WebSocket closed gracefully");
                    }
                    inbound_tx.send(msg).await.ok();
                }
                Err(e) => {
                    warn!("WebSocket disconnected due to error: {}", e);
                }
            }
        }
        debug!("WebSocket connection closed!");
        handle.abort();
    }
}