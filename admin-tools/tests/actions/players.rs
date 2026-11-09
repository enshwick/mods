//! Actions on players and the first server actions: rights, kicks, bans,
//! the allow-list, the message of the day, announcements, teleports, healing,
//! time and weather, and the audit log.

use std::time::Duration;

use enshwick::PlayerKey;
use enshwick::abi::{ActionCall, PlayerInfo};
use enshwick::actions::sheet::decode;
use enshwick::commands::{self, Caller};
use enshwick::console;
use enshwick::events::{SettingsReplaced, ShutdownStarted, WorldLoaded};
use enshwick::test::Installed;
use enshwick::world::PlayerPermissions;

use super::game::{HEALTH, ann, bob, file_until, joins, player, run, scratch, server, server_with};

#[test]
fn a_player_sees_only_the_reads_and_an_administrator_sees_every_action() {
    let dir = scratch("sheet");
    let loader = server("", &dir);
    let ids = |who: &PlayerInfo| -> Vec<String> {
        decode(
            &loader
                .ask_actions(ActionCall::SHEET, who, 0, "", "")
                .unwrap(),
        )
        .into_iter()
        .map(|entry| entry.id)
        .collect()
    };
    assert_eq!(ids(&bob()), ["online", "motd", "status"]);
    assert_eq!(ids(&ann()).len(), 26);
    assert!(run(&loader, &bob(), "kick", "p0000000000000001").starts_with('0'));
}

#[test]
fn the_world_s_rights_give_an_action_to_a_permission_and_take_one_away() {
    let dir = scratch("rights");
    let loader = server(
        "[rights]\nheal = \"CanEditBase\"\nonline = \"nobody\"\n",
        &dir,
    );
    let builder = player(12, 3, "Cid", PlayerInfo::CAN_EDIT_BASE);
    let sheet = decode(
        &loader
            .ask_actions(ActionCall::SHEET, &builder, 0, "", "")
            .unwrap(),
    );
    let ids: Vec<&str> = sheet.iter().map(|entry| entry.id.as_str()).collect();
    assert_eq!(ids, ["heal", "motd", "status"]);
    assert!(
        run(&loader, &ann(), "online", "").starts_with('0'),
        "nobody includes admins"
    );
    assert_eq!(
        console::run("enshwick.admin-tools online").as_deref(),
        Ok("2 online: Ann (0000000000000001, 0 min), Bob (0000000000000002, 0 min)"),
        "the console always may"
    );
    assert_eq!(
        console::run("adl online").as_deref(),
        Ok("2 online: Ann (0000000000000001, 0 min), Bob (0000000000000002, 0 min)"),
        "the alias names the mod on the console"
    );
}

#[test]
fn an_administrator_kicks_a_player_but_not_another_administrator() {
    let dir = scratch("kick");
    let loader = server("", &dir);
    assert_eq!(
        run(&loader, &ann(), "kick", "p0000000000000002"),
        "1Bob was kicked."
    );
    assert_eq!(loader.removed(), [2]);
    let reply = run(&loader, &ann(), "kick", "p0000000000000001");
    assert_eq!(
        reply,
        "0Ann is an administrator or the host and cannot be removed."
    );
}

#[test]
fn an_administrator_bans_a_player_online_and_lifts_the_ban_while_they_are_away() {
    let dir = scratch("ban");
    let loader = server("", &dir);
    assert_eq!(
        run(&loader, &ann(), "banned", "p0000000000000002\tb1"),
        "1Bob is banned."
    );
    assert_eq!(loader.banned(), [PlayerKey::new(2)]);
    let sheet = decode(
        &loader
            .ask_actions(ActionCall::SHEET, &ann(), 0, "", "")
            .unwrap(),
    );
    let banned = sheet.iter().find(|entry| entry.id == "banned").unwrap();
    assert!(
        banned
            .players
            .iter()
            .any(|(key, _, on)| *key == PlayerKey::new(2) && *on),
        "the list keeps Bob while he is away"
    );
    assert_eq!(
        run(&loader, &ann(), "banned", "p0000000000000002\tb0"),
        "1Bob is no longer banned."
    );
    assert_eq!(
        run(&loader, &ann(), "banned", "p0000000000000001\tb1"),
        "0Ann is an administrator or the host and cannot be removed."
    );
}

#[test]
fn a_ban_kept_from_an_earlier_run_bans_the_account_again_when_it_joins() {
    let dir = scratch("kept-ban");
    std::fs::create_dir_all(&dir).unwrap();
    std::fs::write(
        dir.join("accounts.toml"),
        "[[bans]]\nkey = \"0000000000000002\"\nname = \"Bob\"\nby = \"Ann\"\nat = 0\n",
    )
    .unwrap();
    let loader = server("", &dir);
    let sheet = decode(
        &loader
            .ask_actions(ActionCall::SHEET, &ann(), 0, "", "")
            .unwrap(),
    );
    let banned = sheet.iter().find(|entry| entry.id == "banned").unwrap();
    assert!(
        banned
            .players
            .iter()
            .any(|(key, _, on)| *key == PlayerKey::new(2) && *on),
        "the kept ban reads: {:?}",
        banned.players
    );
    joins(&loader, &ann());
    assert!(loader.banned().is_empty());
    joins(&loader, &bob());
    assert_eq!(
        loader.banned(),
        [PlayerKey::new(2)],
        "banned at the door: {:?}",
        loader.logs()
    );
}

#[test]
fn the_allow_list_keeps_out_an_account_not_on_it_and_lets_administrators_in() {
    let dir = scratch("allow");
    let cid = player(12, 3, "Cid", 0);
    let loader = server_with("allowlist = true\n", &dir, &[cid]);
    assert_eq!(
        run(&loader, &ann(), "allowed", "p0000000000000002\tb1"),
        "1Bob may join while the allow-list is on."
    );
    joins(&loader, &bob());
    joins(&loader, &ann());
    assert!(
        loader.removed().is_empty(),
        "Bob is on the list, and administrators always join"
    );
    joins(&loader, &cid);
    assert_eq!(loader.removed(), [3], "Cid is not on the list");
    assert_eq!(
        run(&loader, &ann(), "allowed", "p0000000000000002\tb0"),
        "1Bob is off the allow-list."
    );
}

#[test]
fn a_player_who_joins_reads_the_message_of_the_day_and_can_ask_for_it_again() {
    let dir = scratch("motd");
    let loader = server("motd = \"Welcome to the Isle\"\n", &dir);
    joins(&loader, &bob());
    assert!(
        loader
            .chat()
            .iter()
            .chain(&loader.notices())
            .any(|(key, text)| { *key == PlayerKey::new(2) && text == "Welcome to the Isle" })
    );
    assert_eq!(run(&loader, &bob(), "motd", ""), "1Welcome to the Isle");
}

#[test]
fn a_restart_warning_reaches_every_player_and_stops_at_zero() {
    let dir = scratch("restart");
    let loader = server("restart_text = \"Back soon.\"\n", &dir);
    assert_eq!(
        run(&loader, &ann(), "restart", "n2"),
        "1Every player is warned of a restart in 2 min."
    );
    let said = |loader: &Installed| -> Vec<String> {
        loader
            .chat()
            .into_iter()
            .chain(loader.notices())
            .map(|(_, text)| text)
            .collect()
    };
    assert!(said(&loader).contains(&"The server restarts in 2 min. Back soon.".to_owned()));
    for _ in 0..70 {
        let _ran = loader.tick(Duration::from_secs(1));
    }
    assert!(said(&loader).contains(&"The server restarts in 1 min. Back soon.".to_owned()));
    assert_eq!(
        run(&loader, &ann(), "restart", "n0"),
        "1The restart warning is stopped."
    );
    assert_eq!(
        run(&loader, &ann(), "restart", "n0"),
        "0No restart warning is under way."
    );
}

#[test]
fn announcements_come_in_turn_at_the_world_s_interval() {
    let dir = scratch("announce");
    let loader = server(
        "announcements = [\"Vote\", \"Rules\"]\nannounce_minutes = 2\n",
        &dir,
    );
    let said = |loader: &Installed| -> Vec<String> {
        loader
            .chat()
            .into_iter()
            .chain(loader.notices())
            .filter(|(key, _)| *key == PlayerKey::new(1))
            .map(|(_, text)| text)
            .collect()
    };
    for _ in 0..125 {
        let _ran = loader.tick(Duration::from_secs(1));
    }
    assert_eq!(said(&loader), ["Vote"], "one each two minutes");
    for _ in 0..120 {
        let _ran = loader.tick(Duration::from_secs(1));
    }
    assert_eq!(said(&loader), ["Vote", "Rules"]);
}

#[test]
fn teleports_go_to_bring_and_send_players_and_the_console_has_no_character() {
    let dir = scratch("teleport");
    let loader = server("", &dir);
    assert_eq!(
        run(&loader, &ann(), "bring", "p0000000000000002"),
        "1Bob is on the way to Ann."
    );
    assert_eq!(
        run(
            &loader,
            &ann(),
            "send",
            "p0000000000000001\tp0000000000000002"
        ),
        "1Ann is on the way to Bob."
    );
    assert_eq!(
        run(
            &loader,
            &ann(),
            "place",
            "p0000000000000002\tn100\tn50\tn-20"
        ),
        "1Bob is on the way to 100, 50, -20."
    );
    let moved: Vec<_> = loader
        .teleports()
        .iter()
        .map(|request| request.entity)
        .collect();
    assert_eq!(
        moved,
        [
            enshwick::Entity(11),
            enshwick::Entity(10),
            enshwick::Entity(11)
        ]
    );
    assert_eq!(
        console::run("enshwick.admin-tools goto Bob").as_deref(),
        Ok("Run this as a player in the world: the console has no character.")
    );
}

#[test]
fn heal_restores_a_player_to_their_full_health_and_no_more() {
    let dir = scratch("heal");
    let loader = server("", &dir);
    assert_eq!(
        run(&loader, &ann(), "heal", "p0000000000000002"),
        "1Bob has full health."
    );
    let bytes = loader.component_bytes(bob().entity, HEALTH).unwrap();
    let current = i32::from_le_bytes(bytes[8..12].try_into().unwrap());
    assert_eq!(current, 200);
    assert!(run(&loader, &bob(), "heal", "p0000000000000002").starts_with('0'));
}

#[test]
fn the_time_of_day_and_the_weather_change_from_their_actions() {
    let dir = scratch("world");
    let loader = server("", &dir);
    assert_eq!(
        run(&loader, &ann(), "time", "n6.5"),
        "1The time is now 06:30."
    );
    assert!(loader.daytime().is_some());
    assert_eq!(
        run(&loader, &ann(), "weather", "c1"),
        "1The weather changes to Rain."
    );
    assert_eq!(loader.forecasts().len(), 1);
}

#[test]
fn every_change_goes_to_the_audit_log_and_reads_back_and_reads_stay_out() {
    let dir = scratch("audit");
    let loader = server("", &dir);
    let _reply = run(&loader, &ann(), "online", "");
    let _reply = run(&loader, &ann(), "time", "n6.5");
    let _reply = run(&loader, &ann(), "weather", "c1");
    assert_eq!(
        console::run("enshwick.admin-tools muted Bob on").as_deref(),
        Ok("Bob is muted in chat.")
    );
    let _reply = loader.type_in_chat(&ann(), 0, "/adl muted Bob off");
    let _reply = run(&loader, &ann(), "kick", "p0000000000000002");
    let deadline = std::time::Instant::now() + Duration::from_secs(5);
    let mut read = String::new();
    while std::time::Instant::now() < deadline && !read.contains("\tkick\t") {
        read = run(&loader, &ann(), "audit", "n10");
        std::thread::yield_now();
    }
    let lines: Vec<&str> = read.strip_prefix('1').unwrap_or(&read).lines().collect();
    assert_eq!(lines.len(), 5, "{read}");
    assert!(
        lines.iter().all(|line| line.starts_with("20")),
        "UTC times: {read}"
    );
    for part in [
        "\tinterface\ttime\t6.5\t",
        "\tinterface\tweather\t1\t",
        "\tconsole\tmuted\tBob on\t",
        "\tchat\tmuted\tBob off\t",
        "Ann (0000000000000001)\tinterface\tkick\tBob\tBob was kicked.",
    ] {
        assert!(read.contains(part), "{part}: {read}");
    }
    assert!(!read.contains("\tonline\t"), "reads stay out: {read}");
}

#[test]
fn the_audit_log_is_off_where_the_world_turns_it_off() {
    let dir = scratch("audit-off");
    let loader = server("audit = \"off\"\n", &dir);
    let _reply = run(&loader, &ann(), "kick", "p0000000000000002");
    assert_eq!(
        run(&loader, &ann(), "audit", "n10"),
        "0The audit log is off in this world's settings."
    );
}

#[test]
fn a_settings_change_while_the_world_runs_goes_to_the_audit_log_and_its_load_does_not() {
    let dir = scratch("audit-settings");
    let loader = server("audit = \"on\"\nmotd = \"Hi\"\n", &dir);
    let carried = loader.rules_changed();
    loader.set_settings("settings", "audit = \"on\"\nmotd = \"Welcome\"\n");
    assert!(loader.raise::<SettingsReplaced>(&()) >= 1);
    assert_eq!(loader.rules_changed(), carried, "the rights stayed");
    loader.set_settings(
        "settings",
        "audit = \"on\"\nmotd = \"Welcome\"\n[rights]\nheal = \"everyone\"\n",
    );
    assert!(loader.raise::<SettingsReplaced>(&()) >= 1);
    assert_eq!(
        loader.rules_changed(),
        carried + 1,
        "new rights are carried"
    );
    let deadline = std::time::Instant::now() + Duration::from_secs(5);
    let mut read = String::new();
    while std::time::Instant::now() < deadline && !read.contains("motd") {
        read = run(&loader, &ann(), "audit", "n10");
        std::thread::yield_now();
    }
    assert!(read.contains("settings\tmotd = \"Welcome\""), "{read}");
    assert!(
        !read.contains("\"Hi\""),
        "the world's load is no change: {read}"
    );
}

#[test]
fn commands_under_adl_run_the_actions_from_chat_with_the_player_s_rights() {
    let dir = scratch("chat");
    let loader = server("", &dir);
    assert_eq!(
        loader
            .type_in_chat(&ann(), 0, "/adl muted Bob on")
            .as_deref(),
        Some("1Bob is muted in chat.")
    );
    assert_eq!(loader.muted(), [PlayerKey::new(2)]);
    assert_eq!(
        loader
            .type_in_chat(&bob(), 0, "/admin-tools online")
            .as_deref(),
        Some("12 online: Ann (0000000000000001, 0 min), Bob (0000000000000002, 0 min, muted)")
    );
    assert_eq!(
        loader
            .type_in_chat(&ann(), 0, "/adl muted Bob off")
            .as_deref(),
        Some("1Bob can write in chat again.")
    );
    assert_eq!(
        loader
            .type_in_chat(&ann(), 0, "/adl muted Ann on")
            .as_deref(),
        Some("0Ann is an administrator or the host and cannot be muted.")
    );
    let refused = loader.type_in_chat(&bob(), 0, "/adl kick Ann").unwrap();
    assert!(refused.starts_with('0'), "{refused}");
    assert!(loader.removed().is_empty());
}

#[test]
fn a_remote_caller_runs_only_what_its_permissions_allow_and_nobody_leaves_the_console() {
    let dir = scratch("remote");
    let loader = server("[rights]\nweather = \"nobody\"\n", &dir);
    let bridge = Caller::new("bridge");
    let kicked = commands::run("adl kick Bob", &bridge).unwrap();
    assert!(!kicked.is_done(), "{}", kicked.text());
    assert!(loader.removed().is_empty(), "no permission, no kick");
    let help = commands::run("adl help", &bridge).unwrap();
    assert!(!help.text().contains("kick"), "{}", help.text());

    let moderator = Caller::new("moderator").with(PlayerPermissions::CanKickBan);
    assert!(commands::run("adl kick Bob", &moderator).unwrap().is_done());
    assert_eq!(loader.removed(), [2]);
    let weather = commands::run("adl weather rain", &moderator).unwrap();
    assert!(!weather.is_done(), "nobody: {}", weather.text());

    assert_eq!(
        console::run("adl weather rain").as_deref(),
        Ok("The weather changes to Rain."),
        "the server's console may use an action for nobody"
    );
}

#[test]
fn a_ban_and_its_lifting_reach_the_world_s_file_before_the_world_saves() {
    let dir = scratch("ban-file");
    let loader = server("", &dir);
    assert!(run(&loader, &ann(), "banned", "p0000000000000002\tb1").starts_with('1'));
    let text = file_until(&dir, "accounts.toml", |text| {
        text.contains("0000000000000002")
    });
    assert!(text.contains("0000000000000002"), "{text}");
    assert!(run(&loader, &ann(), "banned", "p0000000000000002\tb0").starts_with('1'));
    let text = file_until(&dir, "accounts.toml", |text| {
        !text.contains("0000000000000002")
    });
    assert!(!text.contains("0000000000000002"), "{text}");
    assert!(run(&loader, &ann(), "allowed", "p0000000000000001\tb1").starts_with('1'));
    let text = file_until(&dir, "accounts.toml", |text| text.contains("[[allowed]]"));
    assert!(text.contains("0000000000000001"), "{text}");
}

#[test]
fn lists_that_do_not_read_ban_no_one_in_the_game_and_keep_the_door_shut() {
    let dir = scratch("lists-shut");
    std::fs::create_dir_all(&dir).unwrap();
    std::fs::write(dir.join("accounts.toml"), "bans = 3\n").unwrap();
    let loader = server("", &dir);
    let reply = run(&loader, &ann(), "banned", "p0000000000000002\tb1");
    assert!(reply.starts_with('0'), "{reply}");
    assert!(
        loader.banned().is_empty(),
        "nothing changed means the game banned no one either"
    );
    joins(&loader, &ann());
    assert!(loader.removed().is_empty(), "administrators always get in");
    joins(&loader, &bob());
    assert_eq!(loader.removed(), [2], "a ban that does not read may hold");
}

#[test]
fn a_restart_warning_and_the_schedule_end_with_the_game_that_started_them() {
    let dir = scratch("restart-ends");
    let loader = server("", &dir);
    assert!(run(&loader, &ann(), "restart", "n2").starts_with('1'));
    assert!(loader.raise::<ShutdownStarted>(&()) >= 1);
    assert!(loader.raise::<WorldLoaded>(&()) >= 1);
    let before = loader.notices().len() + loader.chat().len();
    for _ in 0..130 {
        let _ran = loader.tick(Duration::from_secs(1));
    }
    let said: Vec<String> = loader
        .chat()
        .into_iter()
        .chain(loader.notices())
        .map(|(_, text)| text)
        .collect();
    assert_eq!(said.len(), before, "the next game hears nothing: {said:?}");
    assert_eq!(
        run(&loader, &ann(), "restart", "n0"),
        "0No restart warning is under way."
    );
}

#[test]
fn a_settings_change_that_turns_the_audit_log_off_is_the_last_line_it_writes() {
    let dir = scratch("audit-turned-off");
    let loader = server("audit = \"on\"\n", &dir);
    loader.set_settings("settings", "audit = \"off\"\n");
    assert!(loader.raise::<SettingsReplaced>(&()) >= 1);
    let text = file_until(&dir, "audit.log", |text| text.contains("audit = \"off\""));
    assert!(text.contains("settings\taudit = \"off\""), "{text}");
}
