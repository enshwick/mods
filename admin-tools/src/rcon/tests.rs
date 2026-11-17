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
fn a_new_password_is_long_enough_of_the_alphabet_and_goes_into_a_secret_setting() {
    use enshwick::config::Config as _;
    let first = random_password().unwrap();
    assert_eq!(first.len(), PASSWORD_LEN);
    assert!(first.bytes().all(|b| ALPHABET.contains(&b)));
    assert_ne!(random_password().unwrap(), first);
    let bounds = crate::settings::MIN_PASSWORD..=crate::settings::MAX_PASSWORD;
    assert!(
        bounds.contains(&PASSWORD_LEN),
        "the setting's check takes it"
    );
    let schema = Settings::schema();
    let field = schema.field(PASSWORD_KEY).unwrap();
    assert!(field.secret, "never shown or logged");
}

fn shared(password: &str) -> Shared {
    let config = Config {
        address: SocketAddr::from(([127, 0, 0, 1], 0)),
        password: password.to_owned(),
    };
    Shared::new(config, Weak::new())
}

#[test]
fn failed_logins_count_per_address_across_connections() {
    let shared = shared("pw");
    let (ip, other): (IpAddr, IpAddr) = ("10.0.0.1".parse().unwrap(), "10.0.0.2".parse().unwrap());
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
    let served = std::thread::spawn(move || serve(stream, from.ip(), &shared, None));
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
fn an_ipv6_client_counts_as_its_whole_network_for_logins_and_slots() {
    let shared = shared("");
    let one: IpAddr = "2001:db8:1:2::a".parse().unwrap();
    let same: IpAddr = "2001:db8:1:2:ffff::b".parse().unwrap();
    let other: IpAddr = "2001:db8:1:3::a".parse().unwrap();
    for _ in 0..MAX_FAILURES {
        let _waits = shared.strike(one);
    }
    assert!(shared.waits(same), "another address of the same /64");
    assert!(!shared.waits(other));
    assert!(shared.take_slot(one) && shared.take_slot(same));
    assert!(!shared.take_slot(one), "two from one network");
    assert_eq!(
        network("::1".parse().unwrap()),
        "::1".parse::<IpAddr>().unwrap()
    );
    assert_eq!(
        network("::ffff:10.0.0.1".parse().unwrap()),
        IpAddr::from([10, 0, 0, 1])
    );
}

#[test]
fn a_console_without_a_password_never_listens() {
    let config = Config {
        address: SocketAddr::from(([127, 0, 0, 1], 0)),
        password: String::new(),
    };
    let refused = Listener::open(config, Weak::new()).err().unwrap();
    assert_eq!(refused.kind(), io::ErrorKind::InvalidInput);
}

#[test]
fn a_unit_test_s_console_beyond_loopback_is_refused_before_it_listens() {
    for address in [
        SocketAddr::from(([0, 0, 0, 0], 0)),
        // Refused before the thread starts, so nothing listens.
        // ast-grep-ignore: tests-listen-on-loopback
        SocketAddr::from((Ipv6Addr::UNSPECIFIED, 0)),
    ] {
        let config = Config {
            address,
            password: "a long enough password".into(),
        };
        let refused = Listener::open(config, Weak::new()).err().unwrap();
        assert_eq!(refused.kind(), io::ErrorKind::AddrNotAvailable, "{address}");
    }
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
fn a_new_password_keeps_the_port_and_ends_only_the_sessions_of_the_old_one() {
    const OLD: &str = "the old long password";
    const NEW: &str = "the new long password";
    let port = TcpListener::bind("127.0.0.1:0")
        .unwrap()
        .local_addr()
        .unwrap()
        .port();
    let listener = Listener::open(
        Config {
            address: SocketAddr::from(([127, 0, 0, 1], port)),
            password: OLD.into(),
        },
        Weak::new(),
    )
    .unwrap();
    // A stalled read fails the test instead of waiting out IDLE.
    let connect = |port| {
        let client = connect(port);
        client
            .set_read_timeout(Some(Duration::from_secs(10)))
            .unwrap();
        client
    };
    // The id of the second answer to a login: the packet's own for in, -1 for refused.
    let login = |client: &mut TcpStream, password: &str| {
        let auth = Packet {
            id: 1,
            kind: packet::AUTH,
            body: password.as_bytes().to_vec(),
        };
        client.write_all(&auth.encode()).unwrap();
        let mut answers = [0u8; 28];
        client.read_exact(&mut answers).unwrap();
        i32::from_le_bytes(answers[18..22].try_into().unwrap())
    };
    let mut old = connect(port);
    assert_eq!(login(&mut old, OLD), 1);
    let mut waiting = connect(port);
    let striker = IpAddr::from([10, 0, 0, 9]);
    for _ in 0..MAX_FAILURES {
        let _waits = listener.shared.strike(striker);
    }
    listener.set_password(NEW).unwrap();
    assert!(!listener.shared.waits(striker), "failed logins forgotten");
    let changed = Instant::now();
    let mut rest = Vec::new();
    let _read = old.read_to_end(&mut rest);
    assert!(
        changed.elapsed() < Duration::from_secs(5),
        "the old session is shut down at once, not left to its read timeout"
    );
    assert_eq!(login(&mut waiting, NEW), 1);
    assert_eq!(login(&mut waiting, NEW), 1, "a login again is answered");
    let command = Packet {
        id: 2,
        kind: packet::EXEC,
        body: b"adm online".to_vec(),
    };
    waiting.write_all(&command.encode()).unwrap();
    let mut answer = [0u8; 14];
    waiting.read_exact(&mut answer).unwrap();
    assert_eq!(
        answer[4..8],
        2i32.to_le_bytes(),
        "the new session runs commands"
    );
    let mut fresh = connect(port);
    assert_eq!(login(&mut fresh, OLD), -1);
    let refused = listener.set_password("").unwrap_err();
    assert_eq!(refused.kind(), io::ErrorKind::InvalidInput);
    listener.close();
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
    shared.address = holder.local_addr().unwrap();
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
