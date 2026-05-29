use besfa_ipc::{
    ClientMessage, IpcError, PROTOCOL_VERSION, RuntimeIpcConfig, RuntimeMessage,
    decode_client_message, encode_line, runtime_ready_message,
};
use bevy::prelude::*;
use std::{
    io::{self, BufRead, BufReader, ErrorKind, Write},
    net::{TcpListener, TcpStream},
    thread,
};

pub struct BesfaRuntimeIpcPlugin {
    config: RuntimeIpcConfig,
}

impl BesfaRuntimeIpcPlugin {
    pub const fn new(config: RuntimeIpcConfig) -> Self {
        Self { config }
    }
}

impl Plugin for BesfaRuntimeIpcPlugin {
    fn build(&self, app: &mut App) {
        app.insert_resource(RuntimeIpcServerConfig(self.config))
            .add_systems(Startup, start_runtime_ipc_server);
    }
}

#[derive(Resource)]
struct RuntimeIpcServerConfig(RuntimeIpcConfig);

fn start_runtime_ipc_server(config: Res<RuntimeIpcServerConfig>) {
    let config = config.0;
    let spawn_result = thread::Builder::new()
        .name("besfa-runtime-ipc".into())
        .spawn(move || {
            if let Err(error) = run_ipc_server(config) {
                eprintln!("Besfa runtime IPC stopped: {error}");
            }
        });

    if let Err(error) = spawn_result {
        eprintln!("Besfa runtime IPC thread failed to start: {error}");
    }
}

fn run_ipc_server(config: RuntimeIpcConfig) -> io::Result<()> {
    let listener = TcpListener::bind(("127.0.0.1", config.port))?;

    for stream in listener.incoming() {
        match stream {
            Ok(stream) => {
                if let Err(error) = handle_client(stream, config) {
                    eprintln!("Besfa runtime IPC client error: {error}");
                }
            }
            Err(error) => eprintln!("Besfa runtime IPC accept error: {error}"),
        }
    }

    Ok(())
}

fn handle_client(mut stream: TcpStream, config: RuntimeIpcConfig) -> io::Result<()> {
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

    loop {
        line.clear();
        if reader.read_line(&mut line)? == 0 {
            return Ok(());
        }

        if let Ok(ClientMessage::Command { id, method, .. }) = decode_client_message(&line) {
            write_message(&mut stream, &unsupported_command_response(id, &method))?;
        }
    }
}

fn unsupported_command_response(id: u64, method: &str) -> RuntimeMessage {
    RuntimeMessage::Response {
        id,
        ok: false,
        result: None,
        error: Some(IpcError {
            code: "unsupported_command".to_string(),
            message: format!("Unsupported runtime command: {method}"),
        }),
    }
}

fn write_message(stream: &mut TcpStream, message: &RuntimeMessage) -> io::Result<()> {
    let line =
        encode_line(message).map_err(|error| io::Error::new(ErrorKind::InvalidData, error))?;
    stream.write_all(line.as_bytes())?;
    stream.flush()
}
