//! A dedicated server a game starts from its own menu, and the addresses
//! friends reach it at. [`Hosted::start`] runs the game's own executable
//! as a server with no window, which [`LeaveWithParent`] stops as Ctrl+C
//! would once the game that started it is gone. [`addresses`] finds this
//! computer's address on the local network and on Tailscale, and
//! [`tailnet_peers`] the other computers on Tailscale now.

use std::{
    collections::HashMap,
    fs::{self, File},
    io,
    net::{IpAddr, Ipv4Addr, UdpSocket},
    path::{Path, PathBuf},
    process::{Child, Command, Stdio},
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
    },
};

use bevy::prelude::*;
use serde::Deserialize;

/// Set on a server [`Hosted`] starts, which stops when its stdin closes.
const HOSTED: &str = "BESFA_NET_HOSTED";
/// Tailscale's own DNS address, on every computer it is up on.
const TAILSCALE_DNS: Ipv4Addr = Ipv4Addr::new(100, 100, 100, 100);
/// Where Tailscale's installer puts its command on Windows, also on PATH.
const WINDOWS_TAILSCALE: &str = r"C:\Program Files\Tailscale\tailscale.exe";

/// A dedicated server this game started from its own executable. It stops
/// when this game does, however it ends: dropping this, or the process
/// going, closes the server's stdin.
pub struct Hosted {
    child: Child,
    log: PathBuf,
}

impl Hosted {
    /// Runs this game's executable with `args` as a server: no window,
    /// none of the Besfa editor's variables, so a game Played from the
    /// editor starts a server of its own, and what it prints in `log`.
    pub fn start(args: &[String], log: &Path) -> io::Result<Self> {
        if let Some(folder) = log.parent() {
            fs::create_dir_all(folder)?;
        }
        let out = File::create(log)?;
        let mut command = Command::new(std::env::current_exe()?);
        for (name, _) in std::env::vars_os() {
            if name.to_string_lossy().starts_with("BESFA_") {
                command.env_remove(name);
            }
        }
        command
            .args(args)
            .env(HOSTED, "1")
            .stdin(Stdio::piped())
            .stdout(out.try_clone()?)
            .stderr(out);
        hide(&mut command);
        Ok(Self {
            child: command.spawn()?,
            log: log.to_owned(),
        })
    }

    /// None while the server runs; once it ended, the last error in its
    /// log, or how it ended when there is none.
    pub fn ended(&mut self) -> Option<String> {
        match self.child.try_wait() {
            Ok(None) => None,
            Ok(Some(status)) => Some(
                fs::read_to_string(&self.log)
                    .ok()
                    .and_then(|log| last_error(&log))
                    .unwrap_or_else(|| status.to_string()),
            ),
            Err(error) => Some(error.to_string()),
        }
    }
}

/// The message of the last ERROR line in a game's `log`, its colors and
/// where it came from taken off.
fn last_error(log: &str) -> Option<String> {
    let line = log.lines().rev().map(plain).find(|line| line.contains(" ERROR "))?;
    let message = line.split_once(" ERROR ")?.1;
    // After the module that logged it, as "duskhold::server: ...".
    let message = message.split_once(": ").map_or(message, |(_, after)| after);
    Some(message.trim().to_owned())
}

/// `line` without its terminal color codes, as `\x1b[31m`.
fn plain(line: &str) -> String {
    let mut plain = String::with_capacity(line.len());
    let mut chars = line.chars();
    while let Some(char) = chars.next() {
        if char == '\x1b' {
            chars.by_ref().find(|char| char.is_ascii_alphabetic());
        } else {
            plain.push(char);
        }
    }
    plain
}

/// Stops a server [`Hosted`] started as Ctrl+C would, so it saves, once
/// the game that started it is gone and its stdin closes. A server
/// started any other way, as from a terminal, is left alone. [`headless`]
/// adds it.
///
/// [`headless`]: crate::headless
pub struct LeaveWithParent;

impl Plugin for LeaveWithParent {
    fn build(&self, app: &mut App) {
        if std::env::var_os(HOSTED).is_none() {
            return;
        }
        let gone = Arc::new(AtomicBool::new(false));
        let flag = gone.clone();
        std::thread::spawn(move || {
            // Until the game closes its end, by stopping or by dying.
            let _ = io::copy(&mut io::stdin(), &mut io::sink());
            flag.store(true, Ordering::Relaxed);
        });
        app.insert_resource(ParentGone(gone))
            .add_systems(Update, leave);
    }
}

#[derive(Resource)]
struct ParentGone(Arc<AtomicBool>);

fn leave(gone: Res<ParentGone>, mut exits: MessageWriter<AppExit>, mut left: Local<bool>) {
    if !*left && gone.0.load(Ordering::Relaxed) {
        *left = true;
        exits.write(AppExit::Success);
    }
}

/// Where friends reach this computer: on the local network, and on
/// Tailscale while it is up.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Addresses {
    pub lan: Option<IpAddr>,
    pub tailscale: Option<IpAddr>,
}

/// This computer's addresses, by which way out it takes to the internet
/// and to Tailscale. Nothing is sent.
pub fn addresses() -> Addresses {
    Addresses {
        lan: way_to(Ipv4Addr::new(8, 8, 8, 8)).filter(|ip| !is_tailscale(*ip)),
        tailscale: way_to(TAILSCALE_DNS).filter(|ip| is_tailscale(*ip)),
    }
}

/// This computer's address on the way to `to`: connecting a UDP socket
/// only picks the way.
fn way_to(to: Ipv4Addr) -> Option<IpAddr> {
    let socket = UdpSocket::bind((Ipv4Addr::UNSPECIFIED, 0)).ok()?;
    socket.connect((to, 53)).ok()?;
    let ip = socket.local_addr().ok()?.ip();
    (!ip.is_unspecified() && !ip.is_loopback()).then_some(ip)
}

/// Whether `ip` is in 100.64.0.0/10, the shared space Tailscale gives its
/// computers their addresses from.
pub fn is_tailscale(ip: IpAddr) -> bool {
    matches!(ip, IpAddr::V4(ip) if ip.octets()[0] == 100 && ip.octets()[1] & 0xc0 == 64)
}

/// A computer on this computer's Tailscale network.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Peer {
    pub name: String,
    pub address: IpAddr,
}

/// The other computers on this Tailscale network that are online now, by
/// name; none when Tailscale is not installed or not up. Asks Tailscale's
/// command, which takes a moment.
pub fn tailnet_peers() -> Vec<Peer> {
    let exe = if cfg!(windows) && Path::new(WINDOWS_TAILSCALE).exists() {
        WINDOWS_TAILSCALE
    } else {
        "tailscale"
    };
    let mut command = Command::new(exe);
    command.args(["status", "--json"]);
    hide(&mut command);
    command
        .output()
        .ok()
        .filter(|output| output.status.success())
        .map(|output| peers_in(&String::from_utf8_lossy(&output.stdout)))
        .unwrap_or_default()
}

/// What `tailscale status --json` says, as far as peers go.
#[derive(Deserialize)]
struct Status {
    #[serde(rename = "Peer", default)]
    peer: Option<HashMap<String, StatusPeer>>,
}

#[derive(Deserialize)]
struct StatusPeer {
    #[serde(rename = "HostName", default)]
    host_name: String,
    #[serde(rename = "TailscaleIPs", default)]
    tailscale_ips: Option<Vec<IpAddr>>,
    #[serde(rename = "Online", default)]
    online: bool,
}

/// The online peers in `status`, each by its IPv4 address, sorted by name.
fn peers_in(status: &str) -> Vec<Peer> {
    let Ok(status) = serde_json::from_str::<Status>(status) else {
        return Vec::new();
    };
    let mut peers: Vec<Peer> = status
        .peer
        .unwrap_or_default()
        .into_values()
        .filter(|peer| peer.online)
        .filter_map(|peer| {
            let address = peer.tailscale_ips?.into_iter().find(IpAddr::is_ipv4)?;
            Some(Peer {
                name: peer.host_name,
                address,
            })
        })
        .collect();
    peers.sort_by(|a, b| a.name.cmp(&b.name));
    peers
}

/// No console window for `command` on Windows.
fn hide(command: &mut Command) {
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        const CREATE_NO_WINDOW: u32 = 0x0800_0000;
        command.creation_flags(CREATE_NO_WINDOW);
    }
    #[cfg(not(windows))]
    let _ = command;
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tailscale_addresses_are_the_shared_space() {
        let ip = |text: &str| text.parse::<IpAddr>().unwrap();
        assert!(is_tailscale(ip("100.64.0.1")));
        assert!(is_tailscale(ip("100.101.102.103")));
        assert!(is_tailscale(ip("100.127.255.254")));
        assert!(!is_tailscale(ip("100.128.0.1")));
        assert!(!is_tailscale(ip("100.63.255.255")));
        assert!(!is_tailscale(ip("192.168.0.10")));
        assert!(!is_tailscale(ip("::1")));
        let found = addresses();
        assert!(found.tailscale.is_none_or(is_tailscale));
        assert!(found.lan.is_none_or(|lan| !is_tailscale(lan)));
    }

    #[test]
    fn peers_are_the_online_ones_by_their_ipv4() {
        let status = r#"{
            "Self": {"HostName": "me", "TailscaleIPs": ["100.64.0.1"], "Online": true},
            "Peer": {
                "nodekey:b": {"HostName": "haru-pc", "TailscaleIPs": ["fd7a:115c:a1e0::2", "100.64.0.2"], "Online": true},
                "nodekey:a": {"HostName": "away", "TailscaleIPs": ["100.64.0.3"], "Online": false},
                "nodekey:c": {"HostName": "bora-laptop", "TailscaleIPs": ["100.64.0.4"], "Online": true},
                "nodekey:d": {"HostName": "no-ips", "TailscaleIPs": null, "Online": true}
            }
        }"#;
        assert_eq!(
            peers_in(status),
            [
                Peer {
                    name: "bora-laptop".into(),
                    address: "100.64.0.4".parse().unwrap()
                },
                Peer {
                    name: "haru-pc".into(),
                    address: "100.64.0.2".parse().unwrap()
                },
            ]
        );
        assert_eq!(peers_in(r#"{"Peer": null}"#), []);
        assert_eq!(peers_in("not json"), []);
    }

    #[test]
    fn the_last_error_is_read_without_its_colors() {
        let log = "\x1b[2m2026-10-30T08:57:15Z\x1b[0m \x1b[32m INFO\x1b[0m \x1b[2mgame\x1b[0m\x1b[2m:\x1b[0m opened\n\
                   \x1b[2m2026-10-30T08:57:16Z\x1b[0m \x1b[31mERROR\x1b[0m \x1b[2mgame::server\x1b[0m\x1b[2m:\x1b[0m port 5000: in use\n\
                   \x1b[2m2026-10-30T08:57:17Z\x1b[0m \x1b[32m INFO\x1b[0m \x1b[2mgame\x1b[0m\x1b[2m:\x1b[0m closing\n";
        assert_eq!(last_error(log).as_deref(), Some("port 5000: in use"));
        assert_eq!(last_error("all well\n"), None);
    }
}
