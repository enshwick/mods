//! The remote console: logins, commands, hostile clients, the generated
//! password and when it listens.

use std::time::Duration;

use enshwick::events::{SettingsReplaced, ShutdownStarted};

use super::game::{file_until, hosted, scratch, server, tick_until};

/// A port no program listens on now.
fn free_port() -> u16 {
    std::net::TcpListener::bind("127.0.0.1:0")
        .unwrap()
        .local_addr()
        .unwrap()
        .port()
}

/// One RCON packet on the wire.
fn packet(id: i32, kind: i32, body: &str) -> Vec<u8> {
    let mut bytes = i32::try_from(10 + body.len())
        .unwrap()
        .to_le_bytes()
        .to_vec();
    bytes.extend(id.to_le_bytes());
    bytes.extend(kind.to_le_bytes());
    bytes.extend(body.as_bytes());
    bytes.extend([0, 0]);
    bytes
}

/// Reads one RCON packet: its id, type and body; `None` once the
/// server closed the connection.
fn read_packet(stream: &mut std::net::TcpStream) -> Option<(i32, i32, String)> {
    use std::io::Read as _;
    let mut size = [0u8; 4];
    stream.read_exact(&mut size).ok()?;
    let mut rest = vec![0u8; usize::try_from(i32::from_le_bytes(size)).unwrap()];
    stream.read_exact(&mut rest).ok()?;
    let id = i32::from_le_bytes(rest[..4].try_into().unwrap());
    let kind = i32::from_le_bytes(rest[4..8].try_into().unwrap());
    let body = String::from_utf8_lossy(&rest[8..rest.len() - 2]).into_owned();
    Some((id, kind, body))
}

/// Connects to the remote console on `port`, retrying while it opens.
fn connect(port: u16) -> std::net::TcpStream {
    let deadline = std::time::Instant::now() + Duration::from_secs(5);
    loop {
        match std::net::TcpStream::connect(("127.0.0.1", port)) {
            Ok(stream) => {
                stream
                    .set_read_timeout(Some(Duration::from_secs(10)))
                    .unwrap();
                return stream;
            }
            Err(why) if std::time::Instant::now() > deadline => panic!("{why}"),
            Err(_) => std::thread::sleep(Duration::from_millis(20)),
        }
    }
}

/// The world's own remote console password in these tests.
const PASSWORD: &str = "correct horse battery";

fn rcon_settings(port: u16) -> String {
    format!(
        "rcon = \"on\"\nrcon_ip = \"127.0.0.1\"\nrcon_port = {port}\nrcon_password = \"{PASSWORD}\"\n"
    )
}

#[test]
fn an_rcon_tool_logs_in_and_runs_a_command_with_the_console_s_rights() {
    use std::io::Write as _;
    let dir = scratch("rcon");
    let port = free_port();
    let loader = server(&format!("{}audit = \"on\"\n", rcon_settings(port)), &dir);
    let client = std::thread::spawn(move || {
        let mut stream = connect(port);
        stream.write_all(&packet(5, 3, "nope")).unwrap();
        assert_eq!(read_packet(&mut stream).unwrap(), (5, 0, String::new()));
        assert_eq!(read_packet(&mut stream).unwrap(), (-1, 2, String::new()));
        stream.write_all(&packet(6, 3, PASSWORD)).unwrap();
        assert_eq!(read_packet(&mut stream).unwrap(), (6, 0, String::new()));
        assert_eq!(read_packet(&mut stream).unwrap(), (6, 2, String::new()));
        stream.write_all(&packet(7, 2, "adl online")).unwrap();
        stream.write_all(&packet(8, 0, "")).unwrap();
        let online = read_packet(&mut stream).unwrap();
        let end = read_packet(&mut stream).unwrap();
        stream.write_all(&packet(9, 2, "nothing here")).unwrap();
        let unknown = read_packet(&mut stream).unwrap();
        (online, end, unknown)
    });
    assert!(tick_until(&loader, || client.is_finished()));
    let (online, end, unknown) = client.join().unwrap();
    assert_eq!(
        online,
        (
            7,
            0,
            "2 online: Ann (0000000000000001, 0 min), Bob (0000000000000002, 0 min)".to_owned()
        )
    );
    assert_eq!(end, (8, 0, String::new()), "the end of a long reply");
    assert!(unknown.2.starts_with("Commands: "), "{unknown:?}");
    let audit = file_until(&dir, "audit.log", |text| text.contains("\tok"));
    assert!(
        audit.contains("rcon 127.0.0.1\trcon\tlogin\t\trefused"),
        "{audit}"
    );
    assert!(
        audit.contains("rcon 127.0.0.1\trcon\tlogin\t\tok"),
        "{audit}"
    );
    assert!(!audit.contains(PASSWORD), "{audit}");
}

#[test]
fn rcon_closes_on_a_hostile_size_a_command_before_login_and_repeated_failures() {
    use std::io::Write as _;
    let dir = scratch("rcon-hostile");
    let port = free_port();
    let loader = server(&rcon_settings(port), &dir);
    let client = std::thread::spawn(move || {
        let mut huge = connect(port);
        huge.write_all(&i32::MAX.to_le_bytes()).unwrap();
        let huge = read_packet(&mut huge);

        let mut early = connect(port);
        early.write_all(&packet(1, 2, "adl kick Bob")).unwrap();
        let early = read_packet(&mut early);

        let mut guessing = connect(port);
        for id in 0..3 {
            guessing.write_all(&packet(id, 3, "guess")).unwrap();
            let _empty = read_packet(&mut guessing);
            let _refused = read_packet(&mut guessing);
        }
        let after = read_packet(&mut guessing);
        let mut again = connect(port);
        let _sent = again.write_all(&packet(4, 3, PASSWORD));
        let waiting = read_packet(&mut again);
        (huge, early, after, waiting)
    });
    assert!(tick_until(&loader, || client.is_finished()));
    let (huge, early, after, waiting) = client.join().unwrap();
    assert_eq!(huge, None, "a hostile size closes the connection");
    assert_eq!(early, None, "no command before a login");
    assert_eq!(after, None, "three failures close the connection");
    assert_eq!(waiting, None, "and the address waits");
    assert!(loader.removed().is_empty());
}

#[test]
fn a_dedicated_server_generates_the_rcon_password_and_logs_only_its_file() {
    let dir = scratch("rcon-generated");
    let port = free_port();
    let loader = server(
        &format!("rcon_ip = \"127.0.0.1\"\nrcon_port = {port}\n"),
        &dir,
    );
    let password = std::fs::read_to_string(dir.join("rcon-password.txt")).unwrap();
    let password = password.trim();
    assert_eq!(password.len(), 24);
    let logs = format!("{:?}", loader.logs());
    assert!(logs.contains("rcon-password.txt"), "{logs}");
    assert!(!logs.contains(password), "the password is never logged");
    let _stream = connect(port);
}

#[test]
fn a_hosted_game_opens_no_remote_console_unless_the_world_turns_it_on() {
    let dir = scratch("rcon-hosted");
    let port = free_port();
    let _loader = hosted(
        &format!("rcon_ip = \"127.0.0.1\"\nrcon_port = {port}\n"),
        &dir,
    );
    assert!(std::net::TcpStream::connect(("127.0.0.1", port)).is_err());
    assert!(!dir.join("rcon-password.txt").exists());
}

#[test]
fn the_remote_console_closes_with_the_game_and_moves_when_its_port_changes() {
    let dir = scratch("rcon-moves");
    let (first, second) = (free_port(), free_port());
    let loader = server(&rcon_settings(first), &dir);
    let _open = connect(first);
    loader.set_settings("settings", &rcon_settings(second));
    assert!(loader.raise::<SettingsReplaced>(&()) >= 1);
    let _moved = connect(second);
    assert!(std::net::TcpStream::connect(("127.0.0.1", first)).is_err());
    assert!(loader.raise::<ShutdownStarted>(&()) >= 1);
    assert!(std::net::TcpStream::connect(("127.0.0.1", second)).is_err());
}
