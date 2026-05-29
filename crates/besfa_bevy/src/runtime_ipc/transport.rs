use super::resources::{
    RuntimeIpcClientRegistry, RuntimeIpcCommandRequest, RuntimeIpcServer, RuntimeIpcServerConfig,
};
use besfa_ipc::{
    ClientMessage, IpcError, PROTOCOL_VERSION, RuntimeCommand, RuntimeIpcConfig, RuntimeMessage,
    decode_client_message, encode_line, error_response, runtime_ready_message,
};
use bevy::prelude::*;
use std::{
    io::{self, BufRead, BufReader, ErrorKind, Write},
    net::{TcpListener, TcpStream},
    sync::{
        Arc,
        atomic::{AtomicU64, Ordering},
        mpsc::{self, Receiver, Sender},
    },
    thread,
};

pub(super) fn start_runtime_ipc_server(
    config: Res<RuntimeIpcServerConfig>,
    server: Res<RuntimeIpcServer>,
) {
    let config = config.0;
    let command_tx = server.command_sender();
    let registry = server.registry();
    let snapshot_requests = server.snapshot_requests();
    let spawn_result = thread::Builder::new()
        .name("besfa-runtime-ipc".into())
        .spawn(move || {
            if let Err(error) = run_ipc_server(config, command_tx, registry, snapshot_requests) {
                eprintln!("Besfa runtime IPC stopped: {error}");
            }
        });

    if let Err(error) = spawn_result {
        eprintln!("Besfa runtime IPC thread failed to start: {error}");
    }
}

fn run_ipc_server(
    config: RuntimeIpcConfig,
    command_tx: Sender<RuntimeIpcCommandRequest>,
    registry: RuntimeIpcClientRegistry,
    snapshot_requests: Arc<AtomicU64>,
) -> io::Result<()> {
    let listener = TcpListener::bind(("127.0.0.1", config.port))?;

    for stream in listener.incoming() {
        match stream {
            Ok(stream) => {
                if let Err(error) = handle_client(
                    stream,
                    config,
                    command_tx.clone(),
                    registry.clone(),
                    snapshot_requests.clone(),
                ) {
                    eprintln!("Besfa runtime IPC client error: {error}");
                }
            }
            Err(error) => eprintln!("Besfa runtime IPC accept error: {error}"),
        }
    }

    Ok(())
}

fn handle_client(
    mut stream: TcpStream,
    config: RuntimeIpcConfig,
    command_tx: Sender<RuntimeIpcCommandRequest>,
    registry: RuntimeIpcClientRegistry,
    snapshot_requests: Arc<AtomicU64>,
) -> io::Result<()> {
    stream.set_nodelay(true)?;

    let mut reader = BufReader::new(stream.try_clone()?);
    let mut line = String::new();
    if reader.read_line(&mut line)? == 0 {
        return Ok(());
    }

    match decode_client_message(&line) {
        Ok(ClientMessage::Hello {
            protocol_version,
            token,
        }) if protocol_version == PROTOCOL_VERSION && token == config.token => {
            write_message(&mut stream, &runtime_ready_message())?;
        }
        _ => return Ok(()),
    }

    let (event_tx, event_rx) = mpsc::channel();
    let client_id = registry.register(event_tx.clone());
    snapshot_requests.fetch_add(1, Ordering::Relaxed);

    let writer_stream = stream.try_clone()?;
    thread::Builder::new()
        .name(format!("besfa-runtime-ipc-client-{client_id}"))
        .spawn(move || {
            if let Err(error) = write_events(writer_stream, event_rx) {
                eprintln!("Besfa runtime IPC writer stopped: {error}");
            }
        })?;

    let result = read_client_commands(reader, command_tx, event_tx);
    registry.unregister(client_id);
    result
}

fn read_client_commands(
    mut reader: BufReader<TcpStream>,
    command_tx: Sender<RuntimeIpcCommandRequest>,
    event_tx: Sender<RuntimeMessage>,
) -> io::Result<()> {
    let mut line = String::new();
    loop {
        line.clear();
        if reader.read_line(&mut line)? == 0 {
            return Ok(());
        }

        if let Ok(ClientMessage::Command { id, method, params }) = decode_client_message(&line) {
            match RuntimeCommand::from_method_params(&method, params) {
                Ok(command) => {
                    let send_result = command_tx.send(RuntimeIpcCommandRequest::new(
                        id,
                        command,
                        event_tx.clone(),
                    ));
                    if send_result.is_err() {
                        let _ = event_tx.send(error_response(
                            id,
                            IpcError::new(
                                "runtime_unavailable",
                                "Runtime command queue is unavailable.",
                            ),
                        ));
                    }
                }
                Err(error) => {
                    let _ = event_tx.send(error_response(id, error));
                }
            }
        }
    }
}

fn write_message(stream: &mut TcpStream, message: &RuntimeMessage) -> io::Result<()> {
    let line =
        encode_line(message).map_err(|error| io::Error::new(ErrorKind::InvalidData, error))?;
    stream.write_all(line.as_bytes())?;
    stream.flush()
}

fn write_events(mut stream: TcpStream, event_rx: Receiver<RuntimeMessage>) -> io::Result<()> {
    for message in event_rx {
        write_message(&mut stream, &message)?;
    }

    Ok(())
}
