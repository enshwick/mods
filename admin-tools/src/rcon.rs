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
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering, fence};
use std::sync::mpsc;
use std::sync::{Arc, Mutex, OnceLock, Weak};
use std::time::Instant;

use arc_swap::ArcSwap;
use enshwick::Relock as _;
use enshwick::commands::{self, Caller, RunCommandError};
use rustc_hash::FxHashMap;

use crate::settings::Settings;
use crate::{Tools, texts};
use packet::Packet;

/// The setting that holds the password, which the console writes when it
/// is empty.
const PASSWORD_KEY: &str = "rcon_password";

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
#[derive(Clone)]
pub(crate) struct Config {
    /// The address and port.
    pub address: SocketAddr,
    /// The world's password, never shown or logged.
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
    shared: Arc<Shared>,
}

/// What the listener's threads share.
struct Shared {
    address: SocketAddr,
    /// The password a login takes; a session whose login password no longer
    /// matches it closes.
    password: ArcSwap<String>,
    stop: AtomicBool,
    /// Where the listener took its port, once it has.
    bound: OnceLock<SocketAddr>,
    /// The connections open from each address.
    open: Mutex<FxHashMap<IpAddr, usize>>,
    /// Each client's socket, which closing shuts down.
    clients: Mutex<Vec<Client>>,
    next_client: AtomicU64,
    /// The failed logins of each address within [`PENALTY`].
    strikes: Mutex<FxHashMap<IpAddr, Strikes>>,
    tools: Weak<Tools>,
}

/// A connected client's socket, by a number of its own.
struct Client {
    id: u64,
    stream: TcpStream,
    /// Whether it logged in, so a new password shuts it down.
    logged_in: bool,
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
    /// takes the port.
    ///
    /// # Errors
    ///
    /// The reason no thread starts, or an empty password, which would let
    /// any client log in.
    pub(crate) fn open(config: Config, tools: Weak<Tools>) -> io::Result<Self> {
        refuse_empty(&config.password)?;
        // A unit test on every network raises the firewall prompt.
        if cfg!(test) && !config.address.ip().to_canonical().is_loopback() {
            return Err(io::Error::new(
                io::ErrorKind::AddrNotAvailable,
                "unit tests listen on loopback only",
            ));
        }
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

    /// Where it listens.
    pub(crate) fn address(&self) -> SocketAddr {
        self.shared.address
    }

    /// Makes logins take `password` from now on, keeping the port and the
    /// connections still to log in. A new one shuts down the clients logged
    /// in with the old one and forgets the failed logins, as reopening did;
    /// a client that logged in with the old one meanwhile closes at its next
    /// packet.
    ///
    /// # Errors
    ///
    /// An empty password, which would let any client log in.
    pub(crate) fn set_password(&self, password: &str) -> io::Result<()> {
        refuse_empty(password)?;
        if **self.shared.password.load() == password {
            return Ok(());
        }
        self.shared.password.store(Arc::new(password.to_owned()));
        self.shared.strikes.relock().clear();
        for client in self.shared.clients.relock().iter() {
            if client.logged_in {
                let _shut = client.stream.shutdown(Shutdown::Both);
            }
        }
        enshwick::info!("the remote console takes a new password; its logged-in clients closed");
        Ok(())
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
        for client in self.shared.clients.relock().iter() {
            let _shut = client.stream.shutdown(Shutdown::Both);
        }
    }
}

/// Refuses an empty `password`, which would let any client log in.
fn refuse_empty(password: &str) -> io::Result<()> {
    if password.is_empty() {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            "the remote console has no password",
        ));
    }
    Ok(())
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

/// The listener's thread: takes the port and takes connections until the
/// console closes.
fn listen_on(shared: &Arc<Shared>) {
    let Some(socket) = bind(shared) else {
        return;
    };
    let _set = shared
        .bound
        .set(socket.local_addr().unwrap_or(shared.address));
    fence(Ordering::SeqCst);
    accept(&socket, shared);
}

/// Takes the console's port, again while another socket still holds it;
/// `None` once the console closed or every try failed.
fn bind(shared: &Shared) -> Option<TcpListener> {
    let address = shared.address;
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
                    if let Err(why) = serve(stream, ip, &serving, tracked) {
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
            address: config.address,
            password: ArcSwap::from_pointee(config.password),
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
        let ip = network(ip);
        let mut strikes = self.strikes.relock();
        let now = Instant::now();
        strikes.retain(|_, strikes| now < strikes.last + PENALTY);
        strikes
            .get(&ip)
            .is_some_and(|strikes| strikes.count >= MAX_FAILURES)
    }

    /// Counts a failed login from `ip`; whether the address now waits.
    fn strike(&self, ip: IpAddr) -> bool {
        let ip = network(ip);
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
        let ip = network(ip);
        self.strikes.relock().remove(&ip);
    }

    /// Counts one more connection from `ip` unless [`MAX_CONNECTIONS`] are
    /// open, or [`MAX_PER_ADDRESS`] from `ip`.
    fn take_slot(&self, ip: IpAddr) -> bool {
        let ip = network(ip);
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
        let ip = network(ip);
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
        clients.push(Client {
            id,
            stream: handle,
            logged_in: false,
        });
        Some(id)
    }

    /// Drops the handle of the client `id`.
    fn untrack(&self, id: Option<u64>) {
        if let Some(id) = id {
            self.clients.relock().retain(|client| client.id != id);
        }
    }

    /// Notes whether the client `id` is logged in, so a new password shuts
    /// it down.
    fn mark(&self, id: Option<u64>, logged_in: bool) {
        if let Some(client) = self
            .clients
            .relock()
            .iter_mut()
            .find(|client| Some(client.id) == id)
        {
            client.logged_in = logged_in;
        }
    }

    /// Whether `password`, a session's login password, still holds.
    fn holds(&self, password: &str) -> bool {
        same(password.as_bytes(), self.password.load().as_bytes())
    }

    /// Writes a login to the audit log while it is on.
    fn log_login(&self, label: &str, outcome: &str) {
        if let Some(tools) = self.tools.upgrade() {
            tools.log(&[label, "rcon", "login", "", outcome]);
        }
    }
}

/// The address the limits count `ip` under: an IPv6 address by its /64
/// network, which one client holds whole and can rotate within.
fn network(ip: IpAddr) -> IpAddr {
    match ip {
        IpAddr::V6(v6) if !v6.is_loopback() && v6.to_ipv4_mapped().is_none() => {
            IpAddr::V6(Ipv6Addr::from(v6.to_bits() & !u128::from(u64::MAX)))
        }
        ip => ip.to_canonical(),
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
/// Windows leaves a socket whose receive timed out unusable. `tracked` is
/// the client's number, under which its login is noted.
fn serve(
    mut stream: TcpStream,
    ip: IpAddr,
    shared: &Shared,
    tracked: Option<u64>,
) -> io::Result<()> {
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
    // The password the client logged in with.
    let mut logged_in: Option<Arc<String>> = None;
    while !shared.stop.load(Ordering::SeqCst) {
        let wait = if logged_in.is_some() {
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
                let why = if logged_in.is_some() {
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
                    let password = shared.password.load_full();
                    logged_in = same(&packet.body, password.as_bytes()).then_some(password);
                    shared.mark(tracked, logged_in.is_some());
                    let id = if logged_in.is_some() { packet.id } else { -1 };
                    send(&mut stream, &Packet::answer(packet.id, ""))?;
                    send(
                        &mut stream,
                        &[Packet {
                            id,
                            kind: packet::EXEC,
                            body: Vec::new(),
                        }],
                    )?;
                    shared.log_login(&label, if logged_in.is_some() { "ok" } else { "refused" });
                    if logged_in.is_some() {
                        shared.forgive(ip);
                    } else if shared.strike(ip) {
                        return Err(io::Error::other("too many failed logins"));
                    }
                }
                _ if logged_in.is_none() => {
                    return Err(io::Error::other("a command before a login"));
                }
                // A login racing a new password, which the change did not shut down.
                _ if logged_in.as_ref().is_some_and(|mine| !shared.holds(mine)) => {
                    return Err(io::Error::other("the password changed"));
                }
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
    /// when the address changes; a new password takes effect on the open
    /// port, so no login arriving meanwhile is dropped. With no password, it
    /// closes and writes a new one into the world's setting, whose change
    /// opens it again: that write, once per world, is the one file this
    /// reads or writes, and the listener's thread takes the port.
    pub(crate) fn listen(&self, settings: &Settings) {
        let on = settings.rcon_on(self.dedicated) && !self.rcon_fused.load(Ordering::Acquire);
        let unset = on && settings.rcon_password.is_empty();
        let wanted = (on && !unset).then(|| rcon_config(settings)).flatten();
        {
            let mut slot = self.rcon.relock();
            let alive = slot.as_ref().filter(|listener| !listener.ended());
            match (alive, wanted) {
                (None, None) => {}
                (Some(listener), Some(config)) if listener.address() == config.address => {
                    if let Err(why) = listener.set_password(&config.password) {
                        enshwick::warn!("the remote console keeps its password: {why}");
                    }
                }
                (_, wanted) => {
                    if let Some(old) = slot.take() {
                        old.close();
                    }
                    if let Some(config) = wanted {
                        match Listener::open(config, Weak::clone(&self.me)) {
                            Ok(listener) => *slot = Some(listener),
                            Err(why) => enshwick::warn!("the remote console does not start: {why}"),
                        }
                    }
                }
            }
        }
        // After the lock: the write's settings change comes back here.
        if unset {
            keep_new_password();
        }
    }
}

/// Where the remote console listens and its password; `None` for an
/// address the settings check would refuse.
fn rcon_config(settings: &Settings) -> Option<Config> {
    let ip: IpAddr = settings.rcon_ip.parse().ok()?;
    let port = u16::try_from(settings.rcon_port).ok()?;
    Some(Config {
        address: SocketAddr::new(ip, port),
        password: settings.rcon_password.clone(),
    })
}

/// Makes a password and writes it into the world's `rcon_password`, a
/// secret the log never shows; the console stays closed when it fails.
fn keep_new_password() {
    let written = random_password()
        .map_err(|why| why.to_string())
        .and_then(|password| {
            enshwick::config::set::<Settings>(PASSWORD_KEY, password).map_err(|why| why.to_string())
        });
    match written {
        Ok(()) => enshwick::info!(
            "the remote console's password is new in the world's Admin Tools settings, {PASSWORD_KEY}"
        ),
        Err(why) => enshwick::warn!("the remote console is off: no password written: {why}"),
    }
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
mod tests;
