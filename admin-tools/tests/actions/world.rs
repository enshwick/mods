//! Saves, the status, the world view, the restart, the chat log and backups.

use std::path::PathBuf;
use std::time::Duration;

use enshwick::PlayerKey;
use enshwick::abi::ActionCall;
use enshwick::actions::sheet::decode;
use enshwick::console;
use enshwick::events::{ShutdownStarted, WorldLoaded, WorldSaved, WorldSaving};

use super::game::{Scratch, ann, bob, file_until, hosted, run, said, scratch, server, tick_until};

#[test]
fn the_status_names_the_uptime_and_the_world_for_everyone_and_the_console() {
    let dir = scratch("status");
    let loader = server("", &dir);
    let reply = run(&loader, &bob(), "status", "");
    assert!(
        reply.starts_with("1Up: 0 h 0 min\nWorld: world0\nMods:\n"),
        "{reply}"
    );
    let console = console::run("adm status").unwrap();
    assert!(
        console.starts_with("Up: 0 h 0 min\nWorld: world0\nMods:\n"),
        "{console}"
    );
}

#[test]
fn save_now_asks_the_game_for_a_save_and_only_administrators_may() {
    let dir = scratch("save");
    let loader = server("", &dir);
    assert_eq!(run(&loader, &ann(), "save", ""), "1The world is saving.");
    assert_eq!(loader.saves(), 1);
    assert!(run(&loader, &bob(), "save", "").starts_with('0'));
    assert_eq!(loader.saves(), 1);
}

#[test]
fn the_world_view_says_so_when_the_game_settings_do_not_read() {
    let dir = scratch("world-view");
    let loader = server("", &dir);
    assert_eq!(
        run(&loader, &ann(), "world", ""),
        "0The game settings do not read on this game build."
    );
}

#[test]
fn a_restart_warning_that_runs_out_saves_the_world() {
    let dir = scratch("restart-save");
    let loader = server("", &dir);
    assert!(run(&loader, &ann(), "restart", "n1").starts_with('1'));
    assert_eq!(loader.saves(), 0);
    assert!(tick_until(&loader, || loader.saves() == 1));
    assert!(said(&loader, 2).contains(&"The server restarts now.".to_owned()));
}

#[test]
fn a_hosted_game_offers_no_restart_warning() {
    let dir = scratch("hosted-restart");
    let loader = hosted("", &dir);
    let sheet = decode(
        &loader
            .ask_actions(ActionCall::SHEET, &ann(), 0, "", "")
            .unwrap(),
    );
    assert!(sheet.iter().all(|entry| entry.id != "restart"));
    assert!(sheet.iter().any(|entry| entry.id == "save"));
    assert!(run(&loader, &ann(), "restart", "n5").starts_with('0'));
}

#[test]
fn the_chat_log_keeps_each_line_with_its_sender_and_reads_back() {
    let dir = scratch("chat-log");
    let loader = server("", &dir);
    assert_eq!(run(&loader, &ann(), "chat", "n5"), "1The log is empty.");
    assert_eq!(
        loader.say(PlayerKey::new(2), "hello all").as_deref(),
        Some("hello all")
    );
    let text = file_until(&dir, "chat.log", |text| text.contains("hello all"));
    assert!(
        text.contains("\tBob\t0000000000000002\tsay\thello all"),
        "{text}"
    );
    let read = run(&loader, &ann(), "chat", "n5");
    assert!(
        read.starts_with('1') && read.contains("hello all"),
        "{read}"
    );
    assert!(run(&loader, &bob(), "chat", "n5").starts_with('0'));
}

#[test]
fn the_chat_log_stays_empty_in_a_hosted_game_unless_the_world_turns_it_on() {
    let dir = scratch("chat-hosted");
    let loader = hosted("", &dir);
    let _said = loader.say(PlayerKey::new(2), "quiet");
    assert_eq!(
        run(&loader, &ann(), "chat", "n5"),
        "0The chat log is off in this world's settings."
    );
    std::thread::sleep(Duration::from_millis(100));
    assert!(!dir.join("chat.log").exists());
}

#[test]
fn a_hosted_game_writes_the_chat_log_when_the_world_turns_it_on() {
    let dir = scratch("chat-hosted-on");
    let loader = hosted("chat_log = \"on\"\n", &dir);
    let _said = loader.say(PlayerKey::new(2), "loud");
    assert!(file_until(&dir, "chat.log", |text| text.contains("loud")).contains("loud"));
}

/// A world's save folder with its save and settings, and the mod's data
/// folder in it, where a backup reads from.
fn saves(name: &str) -> (Scratch, PathBuf) {
    let saves = scratch(name);
    let data = saves.join("world0-mod-data").join("admin-tools");
    std::fs::create_dir_all(&data).unwrap();
    std::fs::create_dir_all(saves.join("world0-mod-settings").join("admin-tools")).unwrap();
    std::fs::write(saves.join("world0.ksd"), "the world").unwrap();
    std::fs::write(saves.join("world1.ksd"), "another world").unwrap();
    (saves, data)
}

#[test]
fn a_backup_holds_the_saves_copies_the_world_and_lets_the_saves_go() {
    let (_saves, data) = saves("backup");
    let loader = server("backups_kept = 1\n", &data);
    assert_eq!(
        run(&loader, &ann(), "backup", ""),
        "1The backup has started. You read where it went when it is done."
    );
    assert!(loader.saves_held(), "held at once");
    assert!(
        run(&loader, &ann(), "backup", "").starts_with('0'),
        "one at a time"
    );
    let backups = data.join("backups");
    assert!(tick_until(&loader, || !loader.saves_held()));
    let made: Vec<PathBuf> = std::fs::read_dir(&backups)
        .unwrap()
        .map(|entry| entry.unwrap().path())
        .collect();
    assert_eq!(made.len(), 1, "{made:?}");
    assert_eq!(
        std::fs::read_to_string(made[0].join("world0.ksd")).unwrap(),
        "the world"
    );
    assert!(made[0].join("world0-mod-settings").is_dir());
    assert!(!made[0].join("world1.ksd").exists());
    assert!(!made[0].join("world0-mod-data/admin-tools/backups").exists());
    assert!(tick_until(&loader, || said(&loader, 1)
        .iter()
        .any(|line| line.starts_with("The backup is in "))));
}

#[test]
fn a_game_that_ends_before_its_backup_copies_leaves_the_next_world_free_to_back_up() {
    let (_saves, data) = saves("backup-ended");
    let loader = server("", &data);
    assert!(run(&loader, &ann(), "backup", "").starts_with('1'));
    assert!(loader.raise::<ShutdownStarted>(&()) >= 1);
    assert!(loader.raise::<WorldLoaded>(&()) >= 1);
    let again = run(&loader, &ann(), "backup", "");
    assert!(again.starts_with('1'), "{again}");
}

#[test]
fn a_backup_that_fails_lets_the_saves_go_and_says_why() {
    let (_saves, data) = saves("backup-fails");
    // A file where the backups' folder would be makes the copy fail.
    std::fs::write(data.join("backups"), "in the way").unwrap();
    let loader = server("", &data);
    assert!(run(&loader, &ann(), "backup", "").starts_with('1'));
    assert!(tick_until(&loader, || !loader.saves_held()));
    assert!(tick_until(&loader, || said(&loader, 1)
        .iter()
        .any(|line| line.starts_with("The backup failed"))));
}

#[test]
fn a_backup_is_refused_when_the_data_folder_is_not_beside_a_save() {
    let dir = scratch("backup-elsewhere");
    let loader = server("", &dir);
    assert!(run(&loader, &ann(), "backup", "").starts_with('0'));
    assert!(!loader.saves_held());
}

#[test]
fn a_backup_waits_for_a_save_under_way_and_the_restart_save_waits_for_the_backup() {
    let (_saves, data) = saves("backup-saving");
    let loader = server("", &data);
    assert!(loader.raise::<WorldSaving>(&()) >= 1);
    assert!(run(&loader, &ann(), "backup", "").starts_with('1'));
    assert!(run(&loader, &ann(), "restart", "n1").starts_with('1'));
    assert!(tick_until(&loader, || said(&loader, 2)
        .contains(&"The server restarts now.".to_owned())));
    assert!(loader.saves_held(), "the copy waits for the save to end");
    assert!(!data.join("backups").exists());
    assert_eq!(loader.saves(), 0, "the restart save waits for the backup");
    assert!(loader.raise::<WorldSaved>(&()) >= 1);
    assert!(tick_until(&loader, || loader.saves() == 1));
    assert!(!loader.saves_held());
    assert_eq!(std::fs::read_dir(data.join("backups")).unwrap().count(), 1);
}

#[test]
fn a_backup_waiting_for_a_save_copies_nothing_once_the_world_stops() {
    let (_saves, data) = saves("backup-shutdown");
    let loader = server("", &data);
    assert!(loader.raise::<WorldSaving>(&()) >= 1);
    assert!(run(&loader, &ann(), "backup", "").starts_with('1'));
    // The copy starts a second after the hold, on the game's clock.
    std::thread::sleep(Duration::from_millis(1100));
    let _ran = loader.tick(Duration::from_secs(1));
    std::thread::sleep(Duration::from_millis(200));
    assert!(loader.raise::<ShutdownStarted>(&()) >= 1);
    std::thread::sleep(Duration::from_millis(500));
    assert!(
        !data.join("backups").exists(),
        "not copied while the final save may be written"
    );
}

#[test]
fn the_console_hears_where_a_backup_s_end_is_written() {
    let (_saves, data) = saves("backup-console");
    let loader = server("", &data);
    assert_eq!(
        console::run("adm backup").unwrap(),
        "The backup has started. The server's log names where it went when it is done, and so does the audit log when it is on."
    );
    assert!(tick_until(&loader, || !loader.saves_held()));
}

#[test]
fn the_help_lists_every_action_under_its_group() {
    let dir = scratch("help-pages");
    let _loader = server("", &dir);
    let help = console::run("adm help").unwrap();
    let helped: Vec<&str> = help
        .lines()
        .skip(1)
        .filter(|line| !line.ends_with(':'))
        .filter_map(|line| line.split([' ', ':']).next())
        .collect();
    assert_eq!(helped.len(), 39, "{help}");
    let groups = help.lines().filter(|line| line.ends_with(':')).count();
    assert_eq!(groups, 9, "the actions sit under their groups: {help}");
    // A unit test holds every page to the groups' order and the default rules.
    let listed: Vec<&str> = include_str!("../../README.md")
        .lines()
        .filter_map(|line| line.strip_prefix("| `/adm "))
        .filter_map(|row| row.split([' ', '`']).next())
        .collect();
    assert_eq!(listed, helped, "the README lists the help's order");
}
