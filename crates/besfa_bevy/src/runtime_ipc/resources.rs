use besfa_ipc::{RuntimeCommand, RuntimeIpcConfig, RuntimeMessage};
use bevy::prelude::*;
use std::sync::{
    Arc, Mutex,
    atomic::{AtomicU64, Ordering},
    mpsc::{self, Receiver, Sender, TryRecvError},
};

#[derive(Resource)]
pub(super) struct RuntimeIpcServerConfig(pub(super) RuntimeIpcConfig);

#[derive(Resource)]
pub(crate) struct RuntimeIpcServer {
    registry: RuntimeIpcClientRegistry,
    command_tx: Sender<RuntimeIpcCommandRequest>,
    command_rx: Mutex<Receiver<RuntimeIpcCommandRequest>>,
    snapshot_requests: Arc<AtomicU64>,
}

impl RuntimeIpcServer {
    pub(super) fn new() -> Self {
        let (command_tx, command_rx) = mpsc::channel();
        Self {
            registry: RuntimeIpcClientRegistry::new(),
            command_tx,
            command_rx: Mutex::new(command_rx),
            snapshot_requests: Arc::new(AtomicU64::new(0)),
        }
    }

    pub(super) fn command_sender(&self) -> Sender<RuntimeIpcCommandRequest> {
        self.command_tx.clone()
    }

    pub(super) fn registry(&self) -> RuntimeIpcClientRegistry {
        self.registry.clone()
    }

    pub(super) fn snapshot_requests(&self) -> Arc<AtomicU64> {
        self.snapshot_requests.clone()
    }

    pub(super) fn request_snapshot(&self) {
        self.snapshot_requests.fetch_add(1, Ordering::Relaxed);
    }

    pub(super) fn snapshot_request_count(&self) -> u64 {
        self.snapshot_requests.load(Ordering::Relaxed)
    }

    pub(crate) fn broadcast(&self, message: RuntimeMessage) {
        self.registry.broadcast(message);
    }

    pub(super) fn drain_commands(&self) -> Vec<RuntimeIpcCommandRequest> {
        let Ok(command_rx) = self.command_rx.lock() else {
            return Vec::new();
        };

        let mut commands = Vec::new();
        loop {
            match command_rx.try_recv() {
                Ok(command) => commands.push(command),
                Err(TryRecvError::Empty) => break,
                Err(TryRecvError::Disconnected) => break,
            }
        }
        commands
    }
}

#[derive(Clone)]
pub(super) struct RuntimeIpcClientRegistry {
    clients: Arc<Mutex<Vec<RuntimeIpcClientEntry>>>,
    next_client_id: Arc<AtomicU64>,
}

impl RuntimeIpcClientRegistry {
    fn new() -> Self {
        Self {
            clients: Arc::new(Mutex::new(Vec::new())),
            next_client_id: Arc::new(AtomicU64::new(1)),
        }
    }

    pub(super) fn register(&self, sender: Sender<RuntimeMessage>) -> u64 {
        let client_id = self.next_client_id.fetch_add(1, Ordering::Relaxed);
        if let Ok(mut clients) = self.clients.lock() {
            clients.push(RuntimeIpcClientEntry { client_id, sender });
        }
        client_id
    }

    pub(super) fn unregister(&self, client_id: u64) {
        if let Ok(mut clients) = self.clients.lock() {
            clients.retain(|client| client.client_id != client_id);
        }
    }

    fn broadcast(&self, message: RuntimeMessage) {
        if let Ok(mut clients) = self.clients.lock() {
            clients.retain(|client| client.sender.send(message.clone()).is_ok());
        }
    }
}

#[derive(Clone)]
struct RuntimeIpcClientEntry {
    client_id: u64,
    sender: Sender<RuntimeMessage>,
}

pub(super) struct RuntimeIpcCommandRequest {
    pub(super) id: u64,
    pub(super) command: RuntimeCommand,
    pub(super) response_tx: Sender<RuntimeMessage>,
}

impl RuntimeIpcCommandRequest {
    pub(super) fn new(
        id: u64,
        command: RuntimeCommand,
        response_tx: Sender<RuntimeMessage>,
    ) -> Self {
        Self {
            id,
            command,
            response_tx,
        }
    }
}

#[derive(Resource, Default)]
pub(super) struct RuntimeIpcProject {
    pub(super) path: Option<String>,
}

#[derive(Resource, Default)]
pub(super) struct RuntimeIpcSelection {
    pub(super) selected_entity_id: Option<String>,
}

#[derive(Resource, Default)]
pub(super) struct RuntimeIpcSnapshotCursor {
    pub(super) last_seen: u64,
}

#[derive(Resource, Default)]
pub(super) struct RuntimeIpcFrameStats {
    pub(super) elapsed_secs: f32,
    pub(super) frames: u32,
}

#[derive(Resource, Default)]
pub(super) struct RuntimeIpcEditorCameraState {
    pub(super) elapsed_secs: f32,
    pub(super) last: Option<besfa_ipc::EditorCameraStatePayload>,
}
