use crate::bus::{InboundMessage, OutboundMessage};
use chrono::{DateTime, Utc};
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::{broadcast, mpsc, RwLock};

/// WebSocket connection information
#[derive(Debug, Clone)]
pub struct WebSocketConnection {
    pub id: String,
    pub user_id: String,
    pub connected_at: DateTime<Utc>,
}

/// Web channel for WebSocket-based chat
#[derive(Clone)]
pub struct WebChannel {
    inbound_tx: mpsc::Sender<InboundMessage>,
    outbound_tx: broadcast::Sender<OutboundMessage>,
    connections: Arc<RwLock<HashMap<String, WebSocketConnection>>>,
}

impl WebChannel {
    pub fn new(
        inbound_tx: mpsc::Sender<InboundMessage>,
        outbound_tx: broadcast::Sender<OutboundMessage>,
    ) -> Self {
        Self {
            inbound_tx,
            outbound_tx,
            connections: Arc::new(RwLock::new(HashMap::new())),
        }
    }

    /// Register a new WebSocket connection
    pub async fn register_connection(&self, conn: WebSocketConnection) {
        let mut connections = self.connections.write().await;
        connections.insert(conn.id.clone(), conn);
    }

    /// Unregister a WebSocket connection
    pub async fn unregister_connection(&self, conn_id: &str) {
        let mut connections = self.connections.write().await;
        connections.remove(conn_id);
    }

    /// Get the inbound message sender
    pub fn inbound_sender(&self) -> mpsc::Sender<InboundMessage> {
        self.inbound_tx.clone()
    }

    /// Subscribe to outbound messages
    pub fn subscribe_outbound(&self) -> broadcast::Receiver<OutboundMessage> {
        self.outbound_tx.subscribe()
    }

    /// Get count of active connections
    pub async fn connection_count(&self) -> usize {
        let connections = self.connections.read().await;
        connections.len()
    }
}
