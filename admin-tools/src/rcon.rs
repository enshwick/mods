//! The remote console: a Source RCON server on a thread of its own. A tool
//! logs in with the world's password and sends command lines, such as
//! `adl online`; each runs on the game thread as the server's console runs
//! it, and the reply goes back to the tool.
//!
//! Everything a client sends is bounded: the packet size, the packets a
//! second, the connections at once and the failed logins, after which the
//! address waits a while.

mod packet;

use core::time::Duration;
use std::io::{self, Read as _, Write as _};
use std::net::{IpAddr, SocketAddr, TcpListener, TcpStream};
use std::path::Path;
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::mpsc;
use std::sync::{Arc, Mutex, Weak};
use std::thread::JoinHandle;
use std::time::Instant;

use enshwick::Relock as _;
use enshwick::commands::{self, Caller, RunCommandError};
use rustc_hash::FxHashMap;

use crate::settings::Settings;
use crate::{Tools, texts};
use packet::Packet;

/// The file in the world's data folder that holds the generated password.
pub(crate) const PASSWORD_FILE: &str = "rcon-password.txt";

/// The length of a generated password.
const PASSWORD_LEN: usize = 24;

/// The characters of a generated password.
const ALPHABET: &[u8; 62] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789";

/// Clients connected at once; a client past it is closed at once.
const MAX_CONNECTIONS: usize = 4;

/// Failed logins on one connection before it closes and its address waits.
const MAX_FAILURES: u32 = 3;

/// How long an address waits after too many failed logins.
const PENALTY: Duration = Duration::from_secs(60);

/// Packets one connection may send a second; past it, the connection closes.
const MAX_PACKETS_PER_SECOND: u32 = 20;

/// How long a connection may stay silent before it closes.
const IDLE: Duration = Duration::from_secs(300);

/// How long a command may wait for the game thread before the reply says so.
const COMMAND_TIMEOUT: Duration = Duration::from_secs(10);

/// How often the threads look whether the console closed.
const POLL: Duration = Duration::from_millis(100);

/// Where the remote console listens and the password it takes.
#[derive(Clone, PartialEq, Eq)]
pub(crate) struct Config {
    /// The address and port.
    pub address: SocketAddr,
    /// The password, never shown or logged.
    pub password: String,
}

impl core::fmt::Debug for Config {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.debug_struct("Config")
            .field("address", &self.address)
            .finish_non_exhaustive()
    }
}

/// The remote console while it listens.
#[derive(Debug)]
pub(crate) struct Listener {
    config: Config,
    shared: Arc<Shared>,
    thread: Option<JoinHandle<()>>,
}

/// What the listener's threads share.
struct Shared {
    password: String,
    stop: AtomicBool,
    connections: AtomicUsize,
    /// Addresses that failed to log in too often, and until when they wait.
    waiting: Mutex<FxHashMap<IpAddr, Instant>>,
    tools: Weak<Tools>,
}

impl core::fmt::Debug for Shared {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.debug_struct("Shared").finish_non_exhaustive()
    }
}

impl Listener {
    /// Listens on the address of `config`.
    ///
    /// # Errors
    ///
    /// The reason the port does not open, such as another program on it.
    pub(crate) fn open(config: Config, tools: Weak<Tools>) -> io::Result<Self> {
        let socket = TcpListener::bind(config.address)?;
        socket.set_nonblocking(true)?;
        let shared = Arc::new(Shared {
            password: config.password.clone(),
            stop: AtomicBool::new(false),
            connections: AtomicUsize::new(0),
            waiting: Mutex::new(FxHashMap::default()),
            tools,
        });
        let accepting = Arc::clone(&shared);
        let thread = std::thread::Builder::new()
            .name("admin-tools rcon".into())
            .spawn(move || guarded(&accepting, || accept(&socket, &accepting)))?;
        Ok(Self {
            config,
            shared,
            thread: Some(thread),
        })
    }

    /// Where it listens and the password it takes.
    pub(crate) const fn config(&self) -> &Config {
        &self.config
    }

    /// Stops listening and closes every connection within a poll.
    pub(crate) fn close(mut self) {
        self.shared.stop.store(true, Ordering::Release);
        if let Some(thread) = self.thread.take() {
            let _joined = thread.join();
        }
    }
}

/// Runs `body`; a panic in it turns the remote console off until the mod
/// loads again, and the rest of the mod keeps running.
fn guarded(shared: &Shared, body: impl FnOnce()) {
    if std::panic::catch_unwind(std::panic::AssertUnwindSafe(body)).is_err() {
        shared.stop.store(true, Ordering::Release);
        if let Some(tools) = shared.tools.upgrade() {
            tools.rcon_fused.store(true, Ordering::Release);
        }
        enshwick::warn!("the remote console is off after an error; the rest of Admin Tools runs");
    }
}

/// Takes connections until the console closes.
fn accept(socket: &TcpListener, shared: &Arc<Shared>) {
    while !shared.stop.load(Ordering::Acquire) {
        let (stream, from) = match socket.accept() {
            Ok(accepted) => accepted,
            Err(why) if why.kind() == io::ErrorKind::WouldBlock => {
                std::thread::sleep(POLL);
                continue;
            }
            Err(why) => {
                enshwick::warn!("remote console: a connection failed: {why}");
                std::thread::sleep(POLL);
                continue;
            }
        };
        if shared.waits(from.ip()) || !shared.take_slot() {
            continue;
        }
        let serving = Arc::clone(shared);
        let spawned = std::thread::Builder::new()
            .name("admin-tools rcon client".into())
            .spawn(move || {
                guarded(&serving, || {
                    if let Err(why) = serve(stream, from.ip(), &serving) {
                        enshwick::info!("remote console: {from} closed: {why}");
                    }
                });
                serving.connections.fetch_sub(1, Ordering::AcqRel);
            });
        if let Err(why) = spawned {
            shared.connections.fetch_sub(1, Ordering::AcqRel);
            enshwick::warn!("remote console: no thread for {from}: {why}");
        }
    }
}

impl Shared {
    /// Whether `ip` still waits after failed logins.
    fn waits(&self, ip: IpAddr) -> bool {
        let mut waiting = self.waiting.relock();
        let now = Instant::now();
        waiting.retain(|_, until| *until > now);
        waiting.contains_key(&ip)
    }

    /// Counts one more connection unless [`MAX_CONNECTIONS`] are open.
    fn take_slot(&self) -> bool {
        self.connections
            .try_update(Ordering::AcqRel, Ordering::Acquire, |open| {
                (open < MAX_CONNECTIONS).then_some(open + 1)
            })
            .is_ok()
    }

    /// Writes a login to the audit log while it is on.
    fn log_login(&self, label: &str, outcome: &str) {
        if let Some(tools) = self.tools.upgrade() {
            tools.log(&[label, "rcon", "login", "", outcome]);
        }
    }
}

/// How a connection's packets go, a second at a time.
struct Pace {
    since: Instant,
    count: u32,
}

impl Pace {
    /// Counts a packet; whether the connection stays under
    /// [`MAX_PACKETS_PER_SECOND`].
    fn admit(&mut self) -> bool {
        if self.since.elapsed() >= Duration::from_secs(1) {
            self.since = Instant::now();
            self.count = 0;
        }
        self.count += 1;
        self.count <= MAX_PACKETS_PER_SECOND
    }
}

/// Serves one client until it leaves, breaks a bound or the console
/// closes.
fn serve(mut stream: TcpStream, ip: IpAddr, shared: &Shared) -> io::Result<()> {
    stream.set_nonblocking(false)?;
    stream.set_read_timeout(Some(POLL))?;
    stream.set_nodelay(true)?;
    let label = format!("rcon {ip}");
    let mut buffer = Vec::new();
    let mut chunk = [0u8; 1024];
    let mut last = Instant::now();
    let mut pace = Pace {
        since: Instant::now(),
        count: 0,
    };
    let (mut logged_in, mut failures) = (false, 0);
    while !shared.stop.load(Ordering::Acquire) {
        match stream.read(&mut chunk) {
            Ok(0) => return Ok(()),
            Ok(read) => {
                buffer.extend_from_slice(&chunk[..read]);
                last = Instant::now();
            }
            Err(why)
                if matches!(
                    why.kind(),
                    io::ErrorKind::WouldBlock | io::ErrorKind::TimedOut
                ) =>
            {
                if last.elapsed() >= IDLE {
                    return Err(io::Error::other("silent too long"));
                }
                continue;
            }
            Err(why) => return Err(why),
        }
        while let Some((packet, used)) = packet::parse(&buffer).map_err(io::Error::other)? {
            buffer.drain(..used);
            if !pace.admit() {
                return Err(io::Error::other("too many packets a second"));
            }
            match packet.kind {
                packet::AUTH => {
                    logged_in = same(&packet.body, shared.password.as_bytes());
                    let id = if logged_in { packet.id } else { -1 };
                    send(&mut stream, &Packet::answer(packet.id, ""))?;
                    send(
                        &mut stream,
                        &[Packet {
                            id,
                            kind: packet::EXEC,
                            body: Vec::new(),
                        }],
                    )?;
                    shared.log_login(&label, if logged_in { "ok" } else { "refused" });
                    if !logged_in {
                        failures += 1;
                        if failures >= MAX_FAILURES {
                            shared.waiting.relock().insert(ip, Instant::now() + PENALTY);
                            return Err(io::Error::other("too many failed logins"));
                        }
                    }
                }
                _ if !logged_in => return Err(io::Error::other("a command before a login")),
                packet::EXEC => {
                    let line = String::from_utf8_lossy(&packet.body).into_owned();
                    let reply = run(line, &label, shared);
                    send(&mut stream, &Packet::answer(packet.id, &reply))?;
                }
                // Tools send an empty answer packet after a command to see
                // where a long reply ends: it comes back as it is.
                _ => send(&mut stream, &Packet::answer(packet.id, ""))?,
            }
        }
    }
    Ok(())
}

/// Writes `packets` to the client.
fn send(stream: &mut TcpStream, packets: &[Packet]) -> io::Result<()> {
    for packet in packets {
        stream.write_all(&packet.encode())?;
    }
    stream.flush()
}

/// Whether `given` equals `password`, in a time that depends on `given`'s
/// length alone.
fn same(given: &[u8], password: &[u8]) -> bool {
    let mut differ = given.len() ^ password.len();
    for (at, byte) in given.iter().enumerate() {
        let expected = password.get(at).copied().unwrap_or(!byte);
        differ |= usize::from(byte ^ expected);
    }
    differ == 0
}

/// Runs the command `line` for the client `label` with the server
/// console's rights, on the game thread; the reply, or why there is none.
fn run(line: String, label: &str, shared: &Shared) -> String {
    let Some(tools) = shared.tools.upgrade() else {
        return String::new();
    };
    let (sender, reply) = mpsc::channel();
    let caller = Caller::new(label).with_all();
    commands::run_later(tools.scheduler, line, caller, move |outcome| {
        let _sent = sender.send(outcome);
    });
    match reply.recv_timeout(COMMAND_TIMEOUT) {
        Ok(Ok(outcome)) => outcome.text().to_owned(),
        Ok(Err(RunCommandError::Unknown | RunCommandError::Empty)) => {
            let names: Vec<String> = commands::list()
                .into_iter()
                .map(|set| format!("{} ({})", set.alias, set.name))
                .collect();
            tools.say(
                texts::REPLY_RCON_COMMANDS,
                &[(texts::value::COMMANDS, &names.join(", "))],
            )
        }
        Ok(Err(why)) => why.to_string(),
        Err(_) => tools.say(texts::REPLY_RCON_TIMEOUT, &[]),
    }
}

impl Tools {
    /// Opens, moves or closes the remote console as `settings` say: it
    /// listens while they turn it on and a world runs, and opens again
    /// when the address or the password changes.
    pub(crate) fn listen(&self, settings: &Settings) {
        let wanted = (settings.rcon_on(self.dedicated) && !self.rcon_fused.load(Ordering::Acquire))
            .then(|| self.rcon_config(settings))
            .flatten();
        let mut slot = self.rcon.relock();
        if slot.as_ref().map(Listener::config) == wanted.as_ref() {
            return;
        }
        if let Some(old) = slot.take() {
            old.close();
        }
        let Some(config) = wanted else {
            return;
        };
        let address = config.address;
        match Listener::open(config, Weak::clone(&self.me)) {
            Ok(listener) => {
                enshwick::info!("the remote console listens on {address}");
                *slot = Some(listener);
            }
            Err(why) => enshwick::warn!("the remote console does not listen on {address}: {why}"),
        }
    }

    /// Where the remote console listens and its password: the world's, or
    /// the one generated in its data folder, made on first use.
    fn rcon_config(&self, settings: &Settings) -> Option<Config> {
        let ip: IpAddr = settings.rcon_ip.parse().ok()?;
        let port = u16::try_from(settings.rcon_port).ok()?;
        let password = if settings.rcon_password.is_empty() {
            let file = self.world.data_dir()?.join(PASSWORD_FILE);
            match generated_password(&file) {
                Ok(password) => {
                    enshwick::info!("the remote console's password is in {}", file.display());
                    password
                }
                Err(why) => {
                    enshwick::warn!("the remote console is off: {}: {why}", file.display());
                    return None;
                }
            }
        } else {
            settings.rcon_password.clone()
        };
        Some(Config {
            address: SocketAddr::new(ip, port),
            password,
        })
    }
}

/// The password kept in `file`, made and written there when the file is
/// missing or empty.
///
/// # Errors
///
/// The reason the file does not read or write, or no random bytes.
pub(crate) fn generated_password(file: &Path) -> io::Result<String> {
    if let Some(kept) = enshwick::files::read_to_string_if_present(file)? {
        let kept = kept.trim();
        if !kept.is_empty() {
            return Ok(kept.to_owned());
        }
    }
    let password = random_password()?;
    if let Some(dir) = file.parent() {
        std::fs::create_dir_all(dir)?;
    }
    enshwick::files::write_atomically(file, format!("{password}\n"))?;
    Ok(password)
}

/// A new password of [`PASSWORD_LEN`] characters of [`ALPHABET`], each
/// equally likely.
fn random_password() -> io::Result<String> {
    // The largest multiple of the alphabet's size below 256, so no
    // character comes up more often.
    const FAIR: u8 = 248;
    let mut password = String::with_capacity(PASSWORD_LEN);
    while password.len() < PASSWORD_LEN {
        let mut bytes = [0u8; 64];
        getrandom::fill(&mut bytes).map_err(io::Error::other)?;
        password.extend(
            bytes
                .into_iter()
                .filter(|&byte| byte < FAIR)
                .map(|byte| char::from(ALPHABET[usize::from(byte) % ALPHABET.len()]))
                .take(PASSWORD_LEN - password.len()),
        );
    }
    Ok(password)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_password_check_takes_only_the_exact_password() {
        assert!(same(b"abc", b"abc"));
        assert!(!same(b"abx", b"abc"));
        assert!(!same(b"ab", b"abc"));
        assert!(!same(b"abcd", b"abc"));
        assert!(!same(b"", b"abc"));
        assert!(same(b"", b""));
    }

    #[test]
    fn a_generated_password_is_kept_and_read_back_the_next_time() {
        let dir = std::env::temp_dir().join(format!("admin-tools-rcon-{}", std::process::id()));
        let _gone = std::fs::remove_dir_all(&dir);
        let file = dir.join(PASSWORD_FILE);
        let first = generated_password(&file).unwrap();
        assert_eq!(first.len(), PASSWORD_LEN);
        assert!(first.bytes().all(|b| ALPHABET.contains(&b)));
        assert_eq!(generated_password(&file).unwrap(), first);
        assert_ne!(random_password().unwrap(), first);
        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn a_connection_past_its_packet_rate_is_refused() {
        let mut pace = Pace {
            since: Instant::now(),
            count: 0,
        };
        let admitted = (0..MAX_PACKETS_PER_SECOND + 5)
            .filter(|_| pace.admit())
            .count();
        assert_eq!(admitted, MAX_PACKETS_PER_SECOND as usize);
    }
}
