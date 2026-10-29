//! Dedicated servers for Besfa games. A server is the game itself run
//! [`headless`], holding the world that the games joined to it show. It
//! [`Server::listen`]s on a UDP port and each game [`Client::connect`]s to
//! it, over [`renet`] and its netcode transport. Each frame a game calls
//! `update`, reads and sends renet's messages, then calls `send_packets`.
//! What the two say is the game's own: [`encode`] and [`decode`] put a
//! message in bytes and back.

// ponytail: bevy_renet's update and send systems as methods the game calls
// from its own systems; switch to bevy_renet once it supports this Bevy
// (bevy_renet 5.0 is on Bevy 0.19).

use std::{
    net::{Ipv4Addr, SocketAddr, UdpSocket},
    ops::{Deref, DerefMut},
    time::{Duration, SystemTime},
};

use bevy::{
    app::PluginGroupBuilder,
    prelude::*,
    render::{RenderPlugin, settings::WgpuSettings},
};
pub use renet;
use renet::{ConnectionConfig, RenetClient, RenetServer};
pub use renet_netcode;
use renet_netcode::{
    ClientAuthentication, NetcodeClientTransport, NetcodeServerTransport, ServerAuthentication,
    ServerConfig,
};
use serde::{Serialize, de::DeserializeOwned};

/// `plugins` with no window and no GPU, ticking `hz` times a second, as a
/// dedicated server runs.
pub fn headless(plugins: PluginGroupBuilder, hz: f64) -> PluginGroupBuilder {
    besfa_components::windowless(plugins, hz).set(RenderPlugin {
        render_creation: WgpuSettings {
            backends: None,
            ..default()
        }
        .into(),
        ..default()
    })
}

/// `message` as bytes to send.
pub fn encode(message: &impl Serialize) -> Vec<u8> {
    serde_json::to_vec(message).expect("messages are plain data")
}

/// The message in `bytes`; none when they are not one, as from another
/// version.
pub fn decode<T: DeserializeOwned>(bytes: &[u8]) -> Option<T> {
    serde_json::from_slice(bytes).ok()
}

/// A server's connection: renet's server, which this derefs to, and the
/// UDP transport under it.
pub struct Server {
    server: RenetServer,
    pub transport: NetcodeServerTransport,
}

impl Server {
    /// Listens on `port` of every address for up to `max_clients` games
    /// that say `protocol`.
    pub fn listen(port: u16, protocol: u64, max_clients: usize) -> Result<Self, String> {
        let socket =
            UdpSocket::bind((Ipv4Addr::UNSPECIFIED, port)).map_err(|error| error.to_string())?;
        let config = ServerConfig {
            current_time: now(),
            max_clients,
            protocol_id: protocol,
            public_addresses: socket.local_addr().into_iter().collect(),
            // ponytail: anyone who knows the address may join; signed
            // connect tokens or Steam when servers open beyond friends.
            authentication: ServerAuthentication::Unsecure,
        };
        let transport =
            NetcodeServerTransport::new(config, socket).map_err(|error| error.to_string())?;
        Ok(Self {
            server: RenetServer::new(ConnectionConfig::default()),
            transport,
        })
    }

    /// Takes in what came in the `delta` since the last time, for its
    /// events and messages to be read.
    pub fn update(&mut self, delta: Duration) {
        self.server.update(delta);
        if let Err(error) = self.transport.update(delta, &mut self.server) {
            debug!("{error}");
        }
    }

    /// Sends the messages given since the last time.
    pub fn send_packets(&mut self) {
        self.transport.send_packets(&mut self.server);
    }

    /// Lets every game go at once, as when the server stops.
    pub fn close(&mut self) {
        self.transport.disconnect_all(&mut self.server);
    }
}

impl Deref for Server {
    type Target = RenetServer;

    fn deref(&self) -> &RenetServer {
        &self.server
    }
}

impl DerefMut for Server {
    fn deref_mut(&mut self) -> &mut RenetServer {
        &mut self.server
    }
}

/// A game's connection to a server: renet's client, which this derefs to,
/// and the UDP transport under it.
pub struct Client {
    client: RenetClient,
    pub transport: NetcodeClientTransport,
}

impl Client {
    /// Joins the server at `address`, which listens for `protocol`.
    pub fn connect(address: SocketAddr, protocol: u64) -> Result<Self, String> {
        let socket =
            UdpSocket::bind((Ipv4Addr::UNSPECIFIED, 0)).map_err(|error| error.to_string())?;
        let now = now();
        let authentication = ClientAuthentication::Unsecure {
            protocol_id: protocol,
            // Unique enough among a few friends: the clock's nanoseconds.
            client_id: now.as_nanos() as u64 ^ u64::from(std::process::id()),
            server_addr: address,
            user_data: None,
        };
        let transport = NetcodeClientTransport::new(now, authentication, socket)
            .map_err(|error| error.to_string())?;
        Ok(Self {
            client: RenetClient::new(ConnectionConfig::default()),
            transport,
        })
    }

    /// Takes in what came in the `delta` since the last time, for its
    /// messages to be read.
    pub fn update(&mut self, delta: Duration) {
        self.client.update(delta);
        if let Err(error) = self.transport.update(delta, &mut self.client) {
            debug!("{error}");
        }
    }

    /// Sends the messages given since the last time.
    pub fn send_packets(&mut self) {
        if let Err(error) = self.transport.send_packets(&mut self.client) {
            debug!("{error}");
        }
    }

    /// Tells the server at once that this game goes, as when it closes.
    pub fn close(&mut self) {
        self.transport.disconnect();
    }
}

impl Deref for Client {
    type Target = RenetClient;

    fn deref(&self) -> &RenetClient {
        &self.client
    }
}

impl DerefMut for Client {
    fn deref_mut(&mut self) -> &mut RenetClient {
        &mut self.client
    }
}

fn now() -> Duration {
    SystemTime::now()
        .duration_since(SystemTime::UNIX_EPOCH)
        .unwrap_or_default()
}

#[cfg(test)]
mod tests {
    use renet::{DefaultChannel, ServerEvent};

    use super::*;

    const PROTOCOL: u64 = u64::from_le_bytes(*b"besfanet");
    const STEP: Duration = Duration::from_millis(10);

    /// One frame of both, until `done` or two seconds pass.
    fn run(
        server: &mut Server,
        client: &mut Client,
        mut done: impl FnMut(&mut Server, &mut Client) -> bool,
    ) {
        for _ in 0..200 {
            client.update(STEP);
            server.update(STEP);
            if done(server, client) {
                return;
            }
            client.send_packets();
            server.send_packets();
            std::thread::sleep(STEP);
        }
        panic!("not in two seconds");
    }

    #[test]
    fn a_game_joins_a_server_they_talk_and_it_goes() {
        let mut server = Server::listen(0, PROTOCOL, 1).unwrap();
        let port = server.transport.addresses()[0].port();
        let mut client =
            Client::connect(SocketAddr::from(([127, 0, 0, 1], port)), PROTOCOL).unwrap();
        let mut joined = None;
        run(&mut server, &mut client, |server, client| {
            if let Some(ServerEvent::ClientConnected { client_id }) = server.get_event() {
                joined = Some(client_id);
            }
            joined.is_some() && client.is_connected()
        });
        let id = joined.unwrap();

        client.send_message(DefaultChannel::ReliableOrdered, encode(&"hello"));
        run(&mut server, &mut client, |server, _| {
            server
                .receive_message(id, DefaultChannel::ReliableOrdered)
                .is_some_and(|bytes| decode::<String>(&bytes).as_deref() == Some("hello"))
        });
        server.send_message(id, DefaultChannel::ReliableOrdered, encode(&[1, 2]));
        run(&mut server, &mut client, |_, client| {
            client
                .receive_message(DefaultChannel::ReliableOrdered)
                .is_some_and(|bytes| decode::<[u8; 2]>(&bytes) == Some([1, 2]))
        });
        assert_eq!(decode::<String>(b"not a message"), None);

        client.close();
        run(&mut server, &mut client, |server, _| {
            matches!(
                server.get_event(),
                Some(ServerEvent::ClientDisconnected { .. })
            )
        });
    }
}
