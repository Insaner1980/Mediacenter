use crate::contracts::AppError;
use serde_json::{json, Value};
use std::{
    collections::HashMap,
    sync::{
        atomic::{AtomicU64, Ordering},
        Arc, Mutex,
    },
    time::Duration,
};
use tokio::{
    io::{AsyncReadExt, AsyncWriteExt},
    net::{unix::OwnedWriteHalf, UnixStream},
    sync::{broadcast, oneshot},
};
type Pending = Arc<Mutex<HashMap<u64, oneshot::Sender<Result<Value, AppError>>>>>;
pub struct Client {
    writer: tokio::sync::Mutex<OwnedWriteHalf>,
    pending: Pending,
    next: AtomicU64,
    events: broadcast::Sender<Value>,
    reader: tokio::task::JoinHandle<()>,
    timeout: Duration,
    closed: Arc<Mutex<Option<AppError>>>,
}
struct RequestGuard {
    pending: Pending,
    id: u64,
}
impl Drop for RequestGuard {
    fn drop(&mut self) {
        if let Ok(mut pending) = self.pending.lock() {
            pending.remove(&self.id);
        }
    }
}
impl Drop for Client {
    fn drop(&mut self) {
        self.reader.abort();
    }
}
impl Client {
    pub fn new(stream: UnixStream, timeout: Duration) -> Self {
        let (mut reader, writer) = stream.into_split();
        let pending: Pending = Arc::new(Mutex::new(HashMap::new()));
        let (events, _) = broadcast::channel(128);
        let closed = Arc::new(Mutex::new(None));
        let requests = pending.clone();
        let notify = events.clone();
        let ended = closed.clone();
        let task = tokio::spawn(async move {
            let mut incoming = Vec::new();
            let mut chunk = [0u8; 4096];
            let error = loop {
                let count = match reader.read(&mut chunk).await {
                    Ok(0) | Err(_) => {
                        break AppError::new("player_disconnected", "The player connection closed.")
                    }
                    Ok(n) => n,
                };
                incoming.extend_from_slice(&chunk[..count]);
                let mut failed = None;
                while let Some(end) = incoming.iter().position(|b| *b == b'\n') {
                    if end > 1024 * 1024 {
                        failed = Some(AppError::new(
                            "player_protocol_error",
                            "The player sent an oversized response.",
                        ));
                        break;
                    }
                    let message: Value = match serde_json::from_slice(&incoming[..end]) {
                        Ok(value) => value,
                        Err(_) => {
                            failed = Some(AppError::new(
                                "player_protocol_error",
                                "The player sent an invalid response.",
                            ));
                            break;
                        }
                    };
                    incoming.drain(..=end);
                    if let Some(id) = message.get("request_id").and_then(Value::as_u64) {
                        if let Some(sender) =
                            requests.lock().ok().and_then(|mut map| map.remove(&id))
                        {
                            let result = if message["error"] == "success" {
                                Ok(message["data"].clone())
                            } else {
                                Err(AppError::new(
                                    "player_command_failed",
                                    "The player did not accept this action.",
                                ))
                            };
                            let _ = sender.send(result);
                        }
                    } else if message.get("event").and_then(Value::as_str).is_some() {
                        let _ = notify.send(message);
                    }
                }
                if let Some(error) = failed {
                    break error;
                }
                if incoming.len() > 1024 * 1024 {
                    break AppError::new(
                        "player_protocol_error",
                        "The player sent an oversized response.",
                    );
                }
            };
            if let Ok(mut value) = ended.lock() {
                *value = Some(error.clone());
            }
            if let Ok(mut map) = requests.lock() {
                for (_, sender) in map.drain() {
                    let _ = sender.send(Err(error.clone()));
                }
            }
            let _ = notify.send(json!({"event":"ipc-disconnected","code":error.code}));
        });
        Self {
            writer: tokio::sync::Mutex::new(writer),
            pending,
            next: AtomicU64::new(1),
            events,
            reader: task,
            timeout,
            closed,
        }
    }
    pub fn subscribe(&self) -> broadcast::Receiver<Value> {
        self.events.subscribe()
    }
    pub async fn command(&self, command: Value) -> Result<Value, AppError> {
        if let Some(error) = self
            .closed
            .lock()
            .map_err(|_| AppError::new("player_state_error", "The player state is unavailable."))?
            .clone()
        {
            return Err(error);
        }
        let id = self.next.fetch_add(1, Ordering::Relaxed);
        let (send, receive) = oneshot::channel();
        {
            let mut pending = self.pending.lock().map_err(|_| {
                AppError::new("player_state_error", "The player state is unavailable.")
            })?;
            if pending.len() >= 32 {
                return Err(AppError::new(
                    "player_busy",
                    "Too many player actions are pending.",
                ));
            }
            pending.insert(id, send);
        }
        let _guard = RequestGuard {
            pending: self.pending.clone(),
            id,
        };
        let mut bytes =
            serde_json::to_vec(&json!({"command":command,"request_id":id})).map_err(|_| {
                AppError::new(
                    "player_command_failed",
                    "The player action could not be encoded.",
                )
            })?;
        bytes.push(b'\n');
        tokio::time::timeout(self.timeout, async {
            {
                let mut writer = self.writer.lock().await;
                writer.write_all(&bytes).await.map_err(|_| {
                    AppError::new("player_disconnected", "The player connection closed.")
                })?;
            }
            receive.await.map_err(|_| {
                AppError::new("player_disconnected", "The player connection closed.")
            })?
        })
        .await
        .map_err(|_| AppError::new("player_timeout", "The player did not respond in time."))?
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    use tokio::io::{AsyncBufReadExt, BufReader};

    async fn request(reader: &mut BufReader<tokio::net::unix::OwnedReadHalf>) -> Value {
        let mut line = String::new();
        tokio::time::timeout(Duration::from_secs(3), reader.read_line(&mut line))
            .await
            .expect("client sent a complete request")
            .unwrap();
        serde_json::from_str(&line).unwrap()
    }
    async fn reply(writer: &mut OwnedWriteHalf, id: &Value, data: Value) {
        writer
            .write_all(
                format!(
                    "{}\n",
                    json!({"request_id":id,"error":"success","data":data})
                )
                .as_bytes(),
            )
            .await
            .unwrap();
    }
    fn client_pair(
        timeout: Duration,
    ) -> (
        Arc<Client>,
        BufReader<tokio::net::unix::OwnedReadHalf>,
        OwnedWriteHalf,
    ) {
        let (stream, peer) = UnixStream::pair().unwrap();
        let (reader, writer) = peer.into_split();
        (
            Arc::new(Client::new(stream, timeout)),
            BufReader::new(reader),
            writer,
        )
    }
    #[tokio::test]
    async fn interleaved_events_do_not_consume_replies() {
        let (client, mut reader, mut writer) = client_pair(Duration::from_secs(3));
        let mut events = client.subscribe();
        let server = tokio::spawn(async move {
            let command = request(&mut reader).await;
            writer
                .write_all(b"{\"event\":\"file-loaded\"}\n")
                .await
                .unwrap();
            reply(&mut writer, &command["request_id"], json!(42)).await;
        });
        assert_eq!(
            client
                .command(json!(["get_property", "duration"]))
                .await
                .unwrap(),
            42
        );
        assert_eq!(events.recv().await.unwrap()["event"], "file-loaded");
        server.await.unwrap();
        assert!(client.pending.lock().unwrap().is_empty());
    }
    #[tokio::test]
    async fn concurrent_replies_are_routed_by_id_instead_of_arrival_order() {
        let (client, mut reader, mut writer) = client_pair(Duration::from_secs(3));
        let server = tokio::spawn(async move {
            let first = request(&mut reader).await;
            let second = request(&mut reader).await;
            assert_ne!(first["request_id"], second["request_id"]);
            writer.write_all(b"{\"event\":\"pause\"}\n").await.unwrap();
            reply(
                &mut writer,
                &second["request_id"],
                second["command"][1].clone(),
            )
            .await;
            // A duplicate reply must not consume the still-pending first request.
            reply(&mut writer, &second["request_id"], json!("duplicate")).await;
            reply(
                &mut writer,
                &first["request_id"],
                first["command"][1].clone(),
            )
            .await;
        });
        let (duration, pause) = tokio::join!(
            client.command(json!(["get_property", "duration"])),
            client.command(json!(["get_property", "pause"]))
        );
        assert_eq!(duration.unwrap(), "duration");
        assert_eq!(pause.unwrap(), "pause");
        server.await.unwrap();
        assert!(client.pending.lock().unwrap().is_empty());
    }
    #[tokio::test]
    async fn timeout_releases_pending_request_and_late_reply_cannot_complete_next() {
        let (client, mut reader, mut writer) = client_pair(Duration::from_millis(300));
        let first = {
            let client = client.clone();
            tokio::spawn(async move { client.command(json!(["get_property", "old"])).await })
        };
        let old = request(&mut reader).await;
        assert_eq!(first.await.unwrap().unwrap_err().code, "player_timeout");
        assert!(client.pending.lock().unwrap().is_empty());
        let next = {
            let client = client.clone();
            tokio::spawn(async move { client.command(json!(["get_property", "new"])).await })
        };
        let new = request(&mut reader).await;
        reply(&mut writer, &old["request_id"], json!("stale")).await;
        reply(&mut writer, &new["request_id"], json!("current")).await;
        assert_eq!(next.await.unwrap().unwrap(), "current");
        assert!(client.pending.lock().unwrap().is_empty());
    }
    #[tokio::test]
    async fn cancelling_a_command_removes_its_pending_slot_and_ignores_late_reply() {
        let (client, mut reader, mut writer) = client_pair(Duration::from_secs(3));
        let pending = {
            let client = client.clone();
            tokio::spawn(async move { client.command(json!(["get_property", "cancelled"])).await })
        };
        let old = request(&mut reader).await;
        assert_eq!(client.pending.lock().unwrap().len(), 1);
        pending.abort();
        assert!(pending.await.unwrap_err().is_cancelled());
        assert!(client.pending.lock().unwrap().is_empty());
        let next = {
            let client = client.clone();
            tokio::spawn(async move { client.command(json!(["get_property", "new"])).await })
        };
        let new = request(&mut reader).await;
        reply(&mut writer, &old["request_id"], json!("cancelled result")).await;
        reply(&mut writer, &new["request_id"], json!("new result")).await;
        assert_eq!(next.await.unwrap().unwrap(), "new result");
        assert!(client.pending.lock().unwrap().is_empty());
    }
    #[tokio::test]
    async fn unexpected_eof_fails_every_pending_request_and_future_commands() {
        let (client, mut reader, writer) = client_pair(Duration::from_secs(3));
        let server = tokio::spawn(async move {
            request(&mut reader).await;
            request(&mut reader).await;
            drop(reader);
            drop(writer);
        });
        let (first, second) = tokio::join!(
            client.command(json!(["get_property", "duration"])),
            client.command(json!(["get_property", "pause"]))
        );
        assert_eq!(first.unwrap_err().code, "player_disconnected");
        assert_eq!(second.unwrap_err().code, "player_disconnected");
        assert_eq!(
            client
                .command(json!(["get_property", "volume"]))
                .await
                .unwrap_err()
                .code,
            "player_disconnected"
        );
        assert!(client.pending.lock().unwrap().is_empty());
        server.await.unwrap();
    }
    #[tokio::test]
    async fn malformed_peer_is_reported_after_request_is_received() {
        let (client, mut reader, mut writer) = client_pair(Duration::from_secs(3));
        let server = tokio::spawn(async move {
            request(&mut reader).await;
            writer.write_all(b"not json\n").await.unwrap();
        });
        assert_eq!(
            client
                .command(json!(["get_property", "pause"]))
                .await
                .unwrap_err()
                .code,
            "player_protocol_error"
        );
        assert_eq!(
            client
                .command(json!(["get_property", "volume"]))
                .await
                .unwrap_err()
                .code,
            "player_protocol_error"
        );
        assert!(client.pending.lock().unwrap().is_empty());
        server.await.unwrap();
    }
    #[tokio::test]
    async fn oversized_incoming_lines_are_rejected_with_or_without_newline() {
        for newline in [false, true] {
            let (client, mut reader, mut writer) = client_pair(Duration::from_secs(3));
            let mut events = client.subscribe();
            let server = tokio::spawn(async move {
                request(&mut reader).await;
                let mut payload = b"{\"event\":\"".to_vec();
                payload.extend(std::iter::repeat_n(b'x', 1024 * 1024));
                payload.extend_from_slice(b"\"}");
                if newline {
                    payload.push(b'\n');
                }
                // The peer may get a broken pipe once the bounded reader rejects the line.
                let _ = writer.write_all(&payload).await;
            });
            assert_eq!(
                client
                    .command(json!(["get_property", "pause"]))
                    .await
                    .unwrap_err()
                    .code,
                "player_protocol_error"
            );
            let event = tokio::time::timeout(Duration::from_secs(3), events.recv())
                .await
                .unwrap()
                .unwrap();
            assert_eq!(event["event"], "ipc-disconnected");
            assert_eq!(event["code"], "player_protocol_error");
            assert!(client.pending.lock().unwrap().is_empty());
            server.await.unwrap();
        }
    }
}
