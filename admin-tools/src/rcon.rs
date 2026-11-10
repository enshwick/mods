//! The remote console: a Source RCON server on a thread of its own. A tool
//! logs in with the world's password and sends command lines, such as
//! `adm online`; each runs on the game thread as the server's console runs
//! it, and the reply goes back to the tool.
//!
//! Everything a client sends is bounded: the packet size, the packets a
//! second, the connections at once and from one address, and the failed
//! logins, after which the address waits a while. The threads block on
//! their sockets and closing shuts the sockets down, so an idle console
//! costs nothing and the game thread never waits for it.

mod packet;

use core::time::Duration;
use std::io::{self, Read as _, Write as _};
use std::net::{IpAddr, Ipv4Addr, Ipv6Addr, Shutdown, SocketAddr, TcpListener, TcpStream};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering, fence};
use std::sync::mpsc;
use std::sync::{Arc, Mutex, OnceLock, Weak};
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

/// Clients connected at once from one address, so a few addresses cannot
/// hold every slot.
const MAX_PER_ADDRESS: usize = 2;

/// Failed logins from one address, over any number of connections, before
/// its connection closes and the address waits.
const MAX_FAILURES: u32 = 3;

/// How long an address waits after too many failed logins, and how long a
/// failed login counts against it.
const PENALTY: Duration = Duration::from_secs(60);

/// How long a new connection may take to log in before it closes, so
/// silent clients cannot hold every slot.
const LOGIN_TIME: Duration = if cfg!(test) {
    Duration::from_millis(300)
} else {
    Duration::from_secs(10)
};

/// Packets one connection may send a second; past it, the connection closes.
const MAX_PACKETS_PER_SECOND: u32 = 20;

/// How long a connection may stay silent before it closes.
const IDLE: Duration = Duration::from_secs(300);

/// How long a command may wait for the game thread before the reply says so.
const COMMAND_TIMEOUT: Duration = Duration::from_secs(10);

/// How many times the listener tries to take its port, which a console
/// closed a moment ago may still hold.
const BIND_TRIES: u32 = 10;

/// How long the listener waits between tries to take its port, and after a
/// connection that failed to arrive.
const RETRY: Duration = Duration::from_millis(500);

/// How long closing tries to reach the listener to wake it.
const WAKE: Duration = Duration::from_secs(1);

/// Where the remote console listens and the password it takes.
#[derive(Clone, PartialEq, Eq)]
pub(crate) struct Config {
    /// The address and port.
    pub address: SocketAddr,
    /// The world's password, never shown or logged; empty for the one
    /// generated in `file`.
    pub password: String,
    /// The file of the generated password in the world's data folder.
    pub file: Option<PathBuf>,
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
    shared: Arc<Shared>,
}

/// What the listener's threads share.
struct Shared {
    config: Config,
    /// The password clients log in with, once the listener's thread has
    /// read or made it.
    password: OnceLock<String>,
    stop: AtomicBool,
    /// Where the listener took its port, once it has.
    bound: OnceLock<SocketAddr>,
    /// The connections open from each address.
    open: Mutex<FxHashMap<IpAddr, usize>>,
    /// Each client's socket, which closing shuts down, by a number of its
    /// own.
    clients: Mutex<Vec<(u64, TcpStream)>>,
    next_client: AtomicU64,
    /// The failed logins of each address within [`PENALTY`].
    strikes: Mutex<FxHashMap<IpAddr, Strikes>>,
    tools: Weak<Tools>,
}

/// An address's failed logins and when the last one came.
#[derive(Clone, Copy, Debug)]
struct Strikes {
    count: u32,
    last: Instant,
}

impl core::fmt::Debug for Shared {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.debug_struct("Shared").finish_non_exhaustive()
    }
}

impl Listener {
    /// Starts the remote console of `config` on a thread of its own, which
    /// reads or makes the password and takes the port.
    ///
    /// # Errors
    ///
    /// The reason no thread starts.
    pub(crate) fn open(config: Config, tools: Weak<Tools>) -> io::Result<Self> {
        let shared = Arc::new(Shared::new(config, tools));
        let listening = Arc::clone(&shared);
        std::thread::Builder::new()
            .name("admin-tools rcon".into())
            .spawn(move || {
                guarded(&listening, || listen_on(&listening));
                // A console that never bound counts as closed, so the next
                // change of settings opens it again.
                listening.stop.store(true, Ordering::SeqCst);
            })?;
        Ok(Self { shared })
    }

    /// Whether its thread ended: closed, or it never took its port.
    pub(crate) fn ended(&self) -> bool {
        self.shared.stop.load(Ordering::SeqCst)
    }

    /// Where it listens and the password it takes.
    pub(crate) fn config(&self) -> &Config {
        &self.shared.config
    }

    /// Stops listening and shuts every connection down; the threads end on
    /// their own, so the caller never waits for them.
    pub(crate) fn close(self) {
        self.shared.stop.store(true, Ordering::SeqCst);
        // Pairs with the fence after `bound` is set: one side sees the other.
        fence(Ordering::SeqCst);
        if let Some(&bound) = self.shared.bound.get() {
            // The connect may take up to WAKE; the caller is the game thread.
            let _spawned = std::thread::Builder::new()
                .name("admin-tools rcon wake".into())
                .spawn(move || wake(bound));
        }
        for (_, client) in self.shared.clients.relock().iter() {
            let _shut = client.shutdown(Shutdown::Both);
        }
    }
}

/// Connects to the listener at `bound` once, so its blocking accept returns
/// and sees that the console closed.
fn wake(bound: SocketAddr) {
    let ip = match bound.ip() {
        IpAddr::V4(ip) if ip.is_unspecified() => IpAddr::V4(Ipv4Addr::LOCALHOST),
        IpAddr::V6(ip) if ip.is_unspecified() => IpAddr::V6(Ipv6Addr::LOCALHOST),
        ip => ip,
    };
    let _woken = TcpStream::connect_timeout(&SocketAddr::new(ip, bound.port()), WAKE);
}

/// Runs `body`; a panic in it turns the remote console off until the mod
/// loads again, and the rest of the mod keeps running.
fn guarded(shared: &Shared, body: impl FnOnce()) {
    if std::panic::catch_unwind(std::panic::AssertUnwindSafe(body)).is_err() {
        shared.stop.store(true, Ordering::SeqCst);
        if let Some(tools) = shared.tools.upgrade() {
            tools.rcon_fused.store(true, Ordering::Release);
        }
        enshwick::warn!("the remote console is off after an error; the rest of Admin Tools runs");
    }
}

/// The listener's thread: reads or makes the password, takes the port and
/// takes connections until the console closes.
fn listen_on(shared: &Arc<Shared>) {
    let config = &shared.config;
    let password = match (config.password.is_empty(), &config.file) {
        (false, _) => config.password.clone(),
        (true, Some(file)) => match generated_password(file) {
            Ok(password) => password,
            Err(why) => {
                enshwick::warn!("the remote console is off: {}: {why}", file.display());
                return;
            }
        },
        (true, None) => return,
    };
    let _set = shared.password.set(password);
    let Some(socket) = bind(shared) else {
        return;
    };
    if let (true, Some(file)) = (config.password.is_empty(), &config.file) {
        enshwick::info!("the remote console's password is in {}", file.display());
    }
    let _set = shared
        .bound
        .set(socket.local_addr().unwrap_or(config.address));
    fence(Ordering::SeqCst);
    accept(&socket, shared);
}

/// Takes the console's port, again while another socket still holds it;
/// `None` once the console closed or every try failed.
fn bind(shared: &Shared) -> Option<TcpListener> {
    let address = shared.config.address;
    for tries in 1..=BIND_TRIES {
        if shared.stop.load(Ordering::SeqCst) {
            return None;
        }
        match TcpListener::bind(address) {
            Ok(socket) => {
                enshwick::info!("the remote console listens on {address}");
                return Some(socket);
            }
            Err(why) if tries == BIND_TRIES => {
                enshwick::warn!("the remote console does not listen on {address}: {why}");
            }
            Err(_) => std::thread::sleep(RETRY),
        }
    }
    None
}

/// Takes connections until the console closes.
fn accept(socket: &TcpListener, shared: &Arc<Shared>) {
    while !shared.stop.load(Ordering::SeqCst) {
        let (stream, from) = match socket.accept() {
            Ok(accepted) => accepted,
            Err(why) => {
                enshwick::warn!("remote console: a connection failed: {why}");
                std::thread::sleep(RETRY);
                continue;
            }
        };
        let ip = from.ip();
        if shared.stop.load(Ordering::SeqCst) || shared.waits(ip) || !shared.take_slot(ip) {
            continue;
        }
        let serving = Arc::clone(shared);
        let spawned = std::thread::Builder::new()
            .name("admin-tools rcon client".into())
            .spawn(move || {
                let tracked = serving.track(&stream);
                guarded(&serving, || {
                    if let Err(why) = serve(stream, ip, &serving) {
                        enshwick::info!("remote console: {from} closed: {why}");
                    }
                });
                serving.untrack(tracked);
                serving.free_slot(ip);
            });
        if let Err(why) = spawned {
            shared.free_slot(ip);
            enshwick::warn!("remote console: no thread for {from}: {why}");
        }
    }
}

impl Shared {
    fn new(config: Config, tools: Weak<Tools>) -> Self {
        Self {
            config,
            password: OnceLock::new(),
            stop: AtomicBool::new(false),
            bound: OnceLock::new(),
            open: Mutex::default(),
            clients: Mutex::default(),
            next_client: AtomicU64::new(0),
            strikes: Mutex::default(),
            tools,
        }
    }

    /// Whether `ip` still waits after [`MAX_FAILURES`] failed logins.
    fn waits(&self, ip: IpAddr) -> bool {
        let mut strikes = self.strikes.relock();
        let now = Instant::now();
        strikes.retain(|_, strikes| now < strikes.last + PENALTY);
        strikes
            .get(&ip)
            .is_some_and(|strikes| strikes.count >= MAX_FAILURES)
    }

    /// Counts a failed login from `ip`; whether the address now waits.
    fn strike(&self, ip: IpAddr) -> bool {
        let now = Instant::now();
        let mut strikes = self.strikes.relock();
        let strikes = strikes.entry(ip).or_insert(Strikes {
            count: 0,
            last: now,
        });
        if now >= strikes.last + PENALTY {
            strikes.count = 0;
        }
        strikes.count += 1;
        strikes.last = now;
        strikes.count >= MAX_FAILURES
    }

    /// Forgets the failed logins of `ip` after it logged in.
    fn forgive(&self, ip: IpAddr) {
        self.strikes.relock().remove(&ip);
    }

    /// Counts one more connection from `ip` unless [`MAX_CONNECTIONS`] are
    /// open, or [`MAX_PER_ADDRESS`] from `ip`.
    fn take_slot(&self, ip: IpAddr) -> bool {
        let mut open = self.open.relock();
        let total: usize = open.values().sum();
        let mine = open.get(&ip).copied().unwrap_or(0);
        // Every local tool and tunnel arrives from loopback, which no crowd reaches.
        if total >= MAX_CONNECTIONS || (mine >= MAX_PER_ADDRESS && !ip.is_loopback()) {
            return false;
        }
        open.insert(ip, mine + 1);
        true
    }

    /// Frees the slot of a connection from `ip` that closed.
    fn free_slot(&self, ip: IpAddr) {
        let mut open = self.open.relock();
        if let Some(mine) = open.get_mut(&ip) {
            *mine -= 1;
            if *mine == 0 {
                open.remove(&ip);
            }
        }
    }

    /// Keeps a handle on `stream` for closing to shut it down, and shuts it
    /// down at once when the console closed already; its number, `None`
    /// when no handle could be had.
    fn track(&self, stream: &TcpStream) -> Option<u64> {
        let handle = stream.try_clone().ok()?;
        let id = self.next_client.fetch_add(1, Ordering::Relaxed);
        let mut clients = self.clients.relock();
        if self.stop.load(Ordering::SeqCst) {
            let _shut = handle.shutdown(Shutdown::Both);
        }
        clients.push((id, handle));
        Some(id)
    }

    /// Drops the handle of the client `id`.
    fn untrack(&self, id: Option<u64>) {
        if let Some(id) = id {
            self.clients.relock().retain(|(client, _)| *client != id);
        }
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
/// closes. A read that waited out its time closes the connection, since
/// Windows leaves a socket whose receive timed out unusable.
fn serve(mut stream: TcpStream, ip: IpAddr, shared: &Shared) -> io::Result<()> {
    let Some(password) = shared.password.get() else {
        return Err(io::Error::other("no password yet"));
    };
    // A client that stops reading cannot keep its thread and slot for good.
    stream.set_write_timeout(Some(COMMAND_TIMEOUT))?;
    stream.set_nodelay(true)?;
    let label = format!("rcon {ip}");
    let mut buffer = Vec::new();
    let mut chunk = [0u8; 1024];
    let opened = Instant::now();
    let mut pace = Pace {
        since: opened,
        count: 0,
    };
    let mut logged_in = false;
    while !shared.stop.load(Ordering::SeqCst) {
        let wait = if logged_in {
            IDLE
        } else {
            LOGIN_TIME.saturating_sub(opened.elapsed())
        };
        if wait.is_zero() {
            return Err(io::Error::other("no login in time"));
        }
        stream.set_read_timeout(Some(wait))?;
        match stream.read(&mut chunk) {
            Ok(0) => return Ok(()),
            Ok(read) => buffer.extend_from_slice(&chunk[..read]),
            Err(why)
                if matches!(
                    why.kind(),
                    io::ErrorKind::WouldBlock | io::ErrorKind::TimedOut
                ) =>
            {
                let why = if logged_in {
                    "silent too long"
                } else {
                    "no login in time"
                };
                return Err(io::Error::other(why));
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
                    logged_in = same(&packet.body, password.as_bytes());
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
                    if logged_in {
                        shared.forgive(ip);
                    } else if shared.strike(ip) {
                        return Err(io::Error::other("too many failed logins"));
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
                .map(|set| match set.alias {
                    Some(alias) => format!("{alias} ({})", set.name),
                    None => set.name,
                })
                .collect();
            tools.say(
                texts::REPLY_RCON_COMMANDS,
                &[(texts::value::COMMANDS, &names.join(", "))],
            )
        }
        Ok(Err(why)) => why.to_string(),
        Err(_) => tools.say(
            texts::REPLY_RCON_TIMEOUT,
            &[(
                texts::value::SECONDS,
                &COMMAND_TIMEOUT.as_secs().to_string(),
            )],
        ),
    }
}

impl Tools {
    /// Opens, moves or closes the remote console as `settings` say: it
    /// listens while they turn it on and a world runs, and opens again
    /// when the address or the password changes. Nothing here reads a file
    /// or waits: the listener's thread does that.
    pub(crate) fn listen(&self, settings: &Settings) {
        let wanted = (settings.rcon_on(self.dedicated) && !self.rcon_fused.load(Ordering::Acquire))
            .then(|| self.rcon_config(settings))
            .flatten();
        let mut slot = self.rcon.relock();
        let alive = slot.as_ref().filter(|listener| !listener.ended());
        if alive.map(Listener::config) == wanted.as_ref() {
            return;
        }
        if let Some(old) = slot.take() {
            old.close();
        }
        let Some(config) = wanted else {
            return;
        };
        match Listener::open(config, Weak::clone(&self.me)) {
            Ok(listener) => *slot = Some(listener),
            Err(why) => enshwick::warn!("the remote console does not start: {why}"),
        }
    }

    /// Where the remote console listens and its password: the world's, or
    /// the one generated in its data folder; `None` for neither.
    fn rcon_config(&self, settings: &Settings) -> Option<Config> {
        let ip: IpAddr = settings.rcon_ip.parse().ok()?;
        let port = u16::try_from(settings.rcon_port).ok()?;
        let file = self.world.data_dir().map(|dir| dir.join(PASSWORD_FILE));
        if settings.rcon_password.is_empty() && file.is_none() {
            return None;
        }
        Some(Config {
            address: SocketAddr::new(ip, port),
            password: settings.rcon_password.clone(),
            file,
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

    fn shared(password: &str) -> Shared {
        let config = Config {
            address: SocketAddr::from(([127, 0, 0, 1], 0)),
            password: password.to_owned(),
            file: None,
        };
        let shared = Shared::new(config, Weak::new());
        shared.password.set(password.to_owned()).unwrap();
        shared
    }

    #[test]
    fn failed_logins_count_per_address_across_connections() {
        let shared = shared("pw");
        let (ip, other): (IpAddr, IpAddr) =
            ("10.0.0.1".parse().unwrap(), "10.0.0.2".parse().unwrap());
        for _ in 1..MAX_FAILURES {
            assert!(!shared.strike(ip), "a new connection would not reset it");
        }
        assert!(!shared.waits(ip));
        assert!(shared.strike(ip));
        assert!(shared.waits(ip));
        assert!(!shared.waits(other));
    }

    /// A connected client and the server's end of it, served on a thread.
    fn client(shared: Arc<Shared>) -> (TcpStream, std::thread::JoinHandle<io::Result<()>>) {
        let socket = TcpListener::bind("127.0.0.1:0").unwrap();
        let client = TcpStream::connect(socket.local_addr().unwrap()).unwrap();
        let (stream, from) = socket.accept().unwrap();
        let served = std::thread::spawn(move || serve(stream, from.ip(), &shared));
        (client, served)
    }

    #[test]
    fn a_client_that_does_not_log_in_in_time_loses_its_connection() {
        let (mut client, served) = client(Arc::new(shared("pw")));
        let started = Instant::now();
        let mut rest = Vec::new();
        assert_eq!(client.read_to_end(&mut rest).unwrap_or(0), 0);
        assert!(served.join().unwrap().is_err());
        assert!(
            started.elapsed() < IDLE,
            "closed by the login time, not idleness"
        );
    }

    #[test]
    fn a_client_that_reconnects_after_each_failure_still_waits() {
        let shared = Arc::new(shared("right password"));
        for _ in 0..MAX_FAILURES {
            let (mut client, served) = client(Arc::clone(&shared));
            let login = Packet {
                id: 1,
                kind: packet::AUTH,
                body: b"wrong".to_vec(),
            };
            client.write_all(&login.encode()).unwrap();
            client.shutdown(Shutdown::Write).unwrap();
            let _closed = served.join().unwrap();
        }
        assert!(shared.waits("127.0.0.1".parse().unwrap()));
    }

    #[test]
    fn connections_past_either_limit_get_no_slot_until_one_closes() {
        let shared = shared("");
        let ip = |last: u8| IpAddr::from([10, 0, 0, last]);
        assert!(shared.take_slot(ip(1)) && shared.take_slot(ip(1)));
        assert!(!shared.take_slot(ip(1)), "two from one address");
        assert!(shared.take_slot(ip(2)) && shared.take_slot(ip(3)));
        assert!(!shared.take_slot(ip(4)), "four in all");
        shared.free_slot(ip(1));
        assert!(shared.take_slot(ip(4)));
        assert!(!shared.take_slot(ip(5)));
    }

    #[test]
    fn local_tools_share_every_slot_from_loopback() {
        let shared = shared("");
        let local = IpAddr::from([127, 0, 0, 1]);
        for _ in 0..MAX_CONNECTIONS {
            assert!(shared.take_slot(local));
        }
        assert!(!shared.take_slot(local), "four in all");
    }

    #[test]
    fn a_console_whose_password_file_cannot_be_made_counts_as_ended() {
        let blocker =
            std::env::temp_dir().join(format!("admin-tools-rcon-file-{}", std::process::id()));
        std::fs::write(&blocker, "a file, not a folder").unwrap();
        let config = Config {
            address: SocketAddr::from(([127, 0, 0, 1], 0)),
            password: String::new(),
            file: Some(blocker.join(PASSWORD_FILE)),
        };
        let listener = Listener::open(config, Weak::new()).unwrap();
        let deadline = Instant::now() + Duration::from_secs(5);
        while !listener.ended() && Instant::now() < deadline {
            std::thread::sleep(Duration::from_millis(10));
        }
        assert!(listener.ended());
        std::fs::remove_file(&blocker).unwrap();
    }

    /// A connection to the console on `port`, once its thread listens.
    fn connect(port: u16) -> TcpStream {
        let deadline = Instant::now() + Duration::from_secs(5);
        loop {
            match TcpStream::connect(("127.0.0.1", port)) {
                Ok(stream) => return stream,
                Err(why) if Instant::now() >= deadline => panic!("no console: {why}"),
                Err(_) => std::thread::sleep(Duration::from_millis(20)),
            }
        }
    }

    #[test]
    fn closing_shuts_a_logged_in_client_down_and_the_port_opens_again_at_once() {
        let port = TcpListener::bind("127.0.0.1:0")
            .unwrap()
            .local_addr()
            .unwrap()
            .port();
        let config = Config {
            address: SocketAddr::from(([127, 0, 0, 1], port)),
            password: "a long enough password".into(),
            file: None,
        };
        let first = Listener::open(config.clone(), Weak::new()).unwrap();
        let mut client = connect(port);
        let login = Packet {
            id: 1,
            kind: packet::AUTH,
            body: config.password.clone().into_bytes(),
        };
        client.write_all(&login.encode()).unwrap();
        let mut answer = [0u8; 14];
        client.read_exact(&mut answer).unwrap();
        let closed = Instant::now();
        first.close();
        let mut rest = Vec::new();
        let _read = client.read_to_end(&mut rest);
        assert!(
            closed.elapsed() < Duration::from_secs(5),
            "shut down, not left to idle"
        );
        let second = Listener::open(config, Weak::new()).unwrap();
        let _again = connect(port);
        second.close();
    }

    #[test]
    fn each_tracked_client_gets_its_own_number_and_one_tracked_after_closing_is_shut_down() {
        let server = TcpListener::bind("127.0.0.1:0").unwrap();
        let mut client = TcpStream::connect(server.local_addr().unwrap()).unwrap();
        let (stream, _) = server.accept().unwrap();
        let shared = shared("pw");
        let first = shared.track(&stream);
        let second = shared.track(&stream);
        assert!(first.is_some() && first != second, "{first:?} {second:?}");
        assert_eq!(shared.clients.relock().len(), 2);
        shared.untrack(first);
        assert_eq!(shared.clients.relock().len(), 1);
        shared.untrack(second);
        assert!(shared.clients.relock().is_empty());
        shared.stop.store(true, Ordering::SeqCst);
        let _late = shared.track(&stream);
        client
            .set_read_timeout(Some(Duration::from_secs(5)))
            .unwrap();
        let mut byte = [0u8; 1];
        assert_eq!(client.read(&mut byte).unwrap(), 0, "shut down at once");
    }

    #[test]
    fn the_console_takes_its_port_once_another_socket_lets_it_go() {
        let holder = TcpListener::bind("127.0.0.1:0").unwrap();
        let mut shared = shared("pw");
        shared.config.address = holder.local_addr().unwrap();
        let shared = Arc::new(shared);
        let binding = Arc::clone(&shared);
        let bound = std::thread::spawn(move || bind(&binding));
        std::thread::sleep(RETRY * 2);
        drop(holder);
        assert!(
            bound.join().unwrap().is_some(),
            "retried until the port was free"
        );
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
