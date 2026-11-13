//! The remote console: logins, commands, hostile clients, the generated
//! password and when it listens.

use std::time::Duration;

use enshwick::events::{SettingsReplaced, ShutdownStarted};

use super::game::{file_until, hosted, scratch, server, tick_until};

/// A port no program listens on now.
fn free_port() -> u16 {
    free_ports::<1>()[0]
}

/// `N` different ports no program listens on now.
fn free_ports<const N: usize>() -> [u16; N] {
    let held: [std::net::TcpListener; N] =
        core::array::from_fn(|_| std::net::TcpListener::bind("127.0.0.1:0").unwrap());
    held.map(|socket| socket.local_addr().unwrap().port())
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
#[should_panic(expected = "tests listen on loopback only")]
fn a_test_whose_remote_console_would_listen_on_every_network_fails_before_the_mod_starts() {
    let _game = server(
        // Refused before the mod starts, so nothing listens.
        // ast-grep-ignore: tests-listen-on-loopback
        "rcon = \"on\"\nrcon_ip = \"0.0.0.0\"\n",
        &scratch("rcon-every"),
    );
}

#[test]
#[should_panic(expected = "tests listen on loopback only")]
fn a_settings_change_that_would_move_the_remote_console_off_loopback_fails_before_it_applies() {
    let loader = server("", &scratch("rcon-moved"));
    loader.set_settings("settings", "rcon = \"on\"\nrcon_ip = \"::\"\n");
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
        stream.write_all(&packet(7, 2, "adm online")).unwrap();
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
            "Admin Tools: 2 online\nAnn (0000000000000001, 0 min)\nBob (0000000000000002, 0 min)"
                .to_owned()
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
        early.write_all(&packet(1, 2, "adm kick Bob")).unwrap();
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

/// The remote console password the world's settings hold; `None` for none.
fn kept_password(loader: &enshwick::test::Installed) -> Option<String> {
    let text = loader.settings_text("settings")?;
    let table: toml::Table = text.parse().unwrap();
    let password = table.get("rcon_password")?.as_str()?;
    (!password.is_empty()).then(|| password.to_owned())
}

#[test]
fn a_dedicated_server_writes_a_new_rcon_password_into_its_setting_and_never_logs_it() {
    use std::io::Write as _;
    let dir = scratch("rcon-generated");
    let port = free_port();
    let loader = server(
        &format!("rcon_ip = \"127.0.0.1\"\nrcon_port = {port}\n"),
        &dir,
    );
    let password = kept_password(&loader).expect("the world's setting holds it");
    assert_eq!(password.len(), 24);
    let client = std::thread::spawn({
        let password = password.clone();
        move || {
            let mut stream = connect(port);
            stream.write_all(&packet(1, 3, &password)).unwrap();
            let _empty = read_packet(&mut stream);
            read_packet(&mut stream)
        }
    });
    assert!(tick_until(&loader, || client.is_finished()));
    assert_eq!(client.join().unwrap(), Some((1, 2, String::new())));
    let logs = format!("{:?}", loader.logs());
    assert!(logs.contains("rcon_password"), "{logs}");
    assert!(!logs.contains(&password), "the password is never logged");
    assert!(!dir.join("rcon-password.txt").exists());
}

#[test]
fn a_hosted_game_opens_no_remote_console_unless_the_world_turns_it_on() {
    let dir = scratch("rcon-hosted");
    let port = free_port();
    let loader = hosted(
        &format!("rcon_ip = \"127.0.0.1\"\nrcon_port = {port}\n"),
        &dir,
    );
    assert!(std::net::TcpStream::connect(("127.0.0.1", port)).is_err());
    assert_eq!(kept_password(&loader), None, "no password is made");
}

#[test]
fn the_remote_console_closes_with_the_game_and_moves_when_its_port_changes() {
    let dir = scratch("rcon-moves");
    let [first, second] = free_ports();
    let loader = server(&rcon_settings(first), &dir);
    let _open = connect(first);
    loader.set_settings("settings", &rcon_settings(second));
    assert!(loader.raise::<SettingsReplaced>(&()) >= 1);
    let _moved = connect(second);
    assert!(closes(first));
    assert!(loader.raise::<ShutdownStarted>(&()) >= 1);
    assert!(closes(second));
}

/// The answer to a login with `password` on `port`: id 1 for in, -1 for
/// refused.
fn login(loader: &enshwick::test::Installed, port: u16, password: &str) -> i32 {
    use std::io::Write as _;
    let password = password.to_owned();
    let client = std::thread::spawn(move || {
        let mut stream = connect(port);
        stream.write_all(&packet(1, 3, &password)).unwrap();
        let _empty = read_packet(&mut stream);
        read_packet(&mut stream).map_or(0, |(id, ..)| id)
    });
    assert!(tick_until(loader, || client.is_finished()));
    client.join().unwrap()
}

#[test]
fn clearing_the_rcon_password_closes_the_console_and_opens_it_again_with_a_new_one() {
    let dir = scratch("rcon-cleared");
    let port = free_port();
    let loader = server(&rcon_settings(port), &dir);
    assert_eq!(login(&loader, port, PASSWORD), 1);
    loader.set_settings("settings", &rcon_settings(port).replace(PASSWORD, ""));
    assert!(loader.raise::<SettingsReplaced>(&()) >= 1);
    let new = kept_password(&loader).expect("a new password is written");
    assert_ne!(new, PASSWORD);
    assert_eq!(login(&loader, port, PASSWORD), -1, "the old one is gone");
    assert_eq!(login(&loader, port, &new), 1);
}

/// Whether the port stops taking connections within two seconds: a closed
/// console's thread lets its port go on its own, without the game thread
/// waiting for it.
fn closes(port: u16) -> bool {
    let deadline = std::time::Instant::now() + Duration::from_secs(2);
    while std::time::Instant::now() < deadline {
        if std::net::TcpStream::connect(("127.0.0.1", port)).is_err() {
            return true;
        }
        std::thread::sleep(Duration::from_millis(20));
    }
    false
}
