//! Bans with a reason, accounts that are away or never joined, the message
//! of the day set in game, announcements, the waypoint teleport and the
//! build log.

use enshwick::abi::{BuildingData, Header};
use enshwick::console;
use enshwick::events::{BuildingPlaced, BuildingRemoved, ShutdownStarted, WorldLoaded};
use enshwick::voxels::{EditSource, Voxel, VoxelChange, VoxelMaterial, VoxelPos};
use enshwick::{Entity, ItemId, PlayerKey};

use super::game::{
    ADMIN, ann, bob, file_until, joins, player, run, said, scratch, server, server_with,
};

#[test]
fn a_ban_keeps_its_reason_its_time_and_who_set_it_and_the_list_reads_them() {
    let dir = scratch("ban-reason");
    let loader = server("", &dir);
    assert_eq!(run(&loader, &ann(), "bans", ""), "1Nobody is banned.");
    assert_eq!(
        run(
            &loader,
            &ann(),
            "ban",
            "p0000000000000002\ttEmptied the chests"
        ),
        "1Bob is banned."
    );
    assert_eq!(loader.banned(), [PlayerKey::new(2)]);
    let list = loader.type_in_chat(&ann(), 0, "/adm bans").unwrap();
    let (head, row) = list.split_once('\n').unwrap();
    assert_eq!(head, "1Admin Tools: 1 banned");
    assert!(
        row.starts_with("Bob (0000000000000002): Emptied the chests. By Ann, 20")
            && row.ends_with(" UTC"),
        "{row}"
    );
    let kept = file_until(&dir, "accounts.toml", |text| text.contains("Emptied"));
    assert!(kept.contains("reason = \"Emptied the chests\""), "{kept}");
    assert!(
        run(&loader, &bob(), "bans", "").starts_with('0'),
        "the list is the administrators'"
    );
    assert_eq!(
        run(&loader, &ann(), "banned", "p0000000000000002\tb0"),
        "1Bob is no longer banned."
    );
    assert_eq!(run(&loader, &ann(), "bans", ""), "1Nobody is banned.");
}

#[test]
fn a_moderator_cannot_ban_or_kick_themselves_where_the_game_would_let_them() {
    let dir = scratch("ban-self");
    let rights = "[rights]\nban = \"everyone\"\nbanned = \"everyone\"\naccount = \"everyone\"\nkick = \"everyone\"\n";
    let loader = server(rights, &dir);
    let refused = "0You cannot do that to yourself.";
    assert_eq!(
        run(&loader, &bob(), "ban", "p0000000000000002\ttoops"),
        refused
    );
    assert_eq!(
        run(&loader, &bob(), "banned", "p0000000000000002\tb1"),
        refused
    );
    assert_eq!(run(&loader, &bob(), "account", "t2\tc0"), refused);
    assert_eq!(run(&loader, &bob(), "kick", "p0000000000000002"), refused);
    assert!(loader.banned().is_empty() && loader.removed().is_empty());
    // Lifting a ban on oneself changes nothing the game would refuse.
    assert_eq!(
        run(&loader, &bob(), "banned", "p0000000000000002\tb0"),
        "1Bob is no longer banned."
    );
}

#[test]
fn a_player_who_is_away_is_banned_by_the_name_they_last_joined_with() {
    let dir = scratch("ban-away");
    std::fs::create_dir_all(&dir).unwrap();
    std::fs::write(
        dir.join("accounts.toml"),
        "bans = []\nallowed = []\n[[seen]]\nkey = \"0000000000000009\"\nname = \"Dee\"\n",
    )
    .unwrap();
    let loader = server("", &dir);
    assert_eq!(
        loader
            .type_in_chat(&ann(), 0, "/adm ban Dee Tore down the hall")
            .as_deref(),
        Some("1Dee is banned.")
    );
    assert!(loader.banned().is_empty(), "the game bans only at the door");
    assert!(
        run(&loader, &ann(), "bans", "")
            .contains("Dee (0000000000000009): Tore down the hall. By Ann")
    );
    assert_eq!(
        loader
            .type_in_chat(&ann(), 0, "/adm allowed Dee on")
            .as_deref(),
        Some("1Dee may join while the allow-list is on.")
    );
    assert_eq!(
        loader
            .type_in_chat(&ann(), 0, "/adm ban Nobody x")
            .as_deref(),
        Some("0No player named Nobody.")
    );
}

#[test]
fn the_door_remembers_who_came_so_a_refused_account_can_be_allowed_by_name() {
    let dir = scratch("door");
    let cid = player(12, 3, "Cid", 0);
    let loader = server_with("allowlist = true\n", &dir, &[cid]);
    joins(&loader, &cid);
    assert_eq!(loader.removed(), [3], "not on the list yet");
    let kept = file_until(&dir, "accounts.toml", |text| text.contains("Cid"));
    assert!(kept.contains("[[seen]]"), "{kept}");
    assert_eq!(
        console::run("enshwick.admin-tools allowed Cid on").as_deref(),
        Ok("Cid may join while the allow-list is on.")
    );
}

#[test]
fn an_account_that_never_joined_is_banned_and_allowed_by_its_id() {
    let dir = scratch("account");
    let eve = player(13, 7, "Eve", 0);
    let loader = server_with("allowlist = true\n", &dir, &[eve]);
    assert_eq!(
        run(&loader, &ann(), "account", "tABCDEF\tc0"),
        "10000000000abcdef is banned."
    );
    assert_eq!(
        console::run("enshwick.admin-tools account abcdef unban").as_deref(),
        Ok("0000000000abcdef is no longer banned.")
    );
    assert_eq!(
        run(&loader, &ann(), "account", "tabcdef\tc2"),
        "10000000000abcdef may join while the allow-list is on."
    );
    // The ban's earlier write names the account too, so wait for the allow-list's.
    let kept = file_until(&dir, "accounts.toml", |text| {
        text.contains("[[allowed]]") && text.contains("abcdef")
    });
    assert!(!kept.contains("[[bans]]"), "{kept}");
    assert_eq!(
        run(&loader, &ann(), "account", "t7\tc2"),
        "1Eve may join while the allow-list is on."
    );
    joins(&loader, &eve);
    assert!(loader.removed().is_empty(), "allowed by her id");
    assert_eq!(
        run(&loader, &ann(), "account", "t7\tc3"),
        "1Eve is off the allow-list.",
        "an account online reads by its name"
    );
    assert!(
        run(&loader, &ann(), "account", "t12345678901234567\tc0").starts_with('0'),
        "the platform refuses more than 16 digits"
    );
    for refused in ["txyz", "t0", "t+7"] {
        let reply = run(&loader, &ann(), "account", &format!("{refused}\tc0"));
        assert!(
            reply.starts_with('0') && reply.contains("is no account id"),
            "{reply}"
        );
    }
}

#[test]
fn a_message_of_the_day_set_in_game_holds_over_the_setting_until_it_is_taken_away() {
    let dir = scratch("setmotd");
    let loader = server("motd = \"Hi\"\n", &dir);
    assert_eq!(
        loader
            .type_in_chat(&ann(), 0, "/adm setmotd Welcome back, all")
            .as_deref(),
        Some("1Players now read when they join: Welcome back, all")
    );
    assert_eq!(run(&loader, &bob(), "motd", ""), "1Welcome back, all");
    joins(&loader, &bob());
    assert!(said(&loader, 2).contains(&"Welcome back, all".to_owned()));
    assert!(run(&loader, &bob(), "setmotd", "tMine").starts_with('0'));
    // It holds over a restart: the world's data keeps it.
    let _kept = file_until(&dir, "accounts.toml", |text| text.contains("Welcome back"));
    assert!(loader.raise::<ShutdownStarted>(&()) >= 1);
    assert!(loader.raise::<WorldLoaded>(&()) >= 1);
    assert_eq!(run(&loader, &bob(), "motd", ""), "1Welcome back, all");
    assert_eq!(
        run(&loader, &ann(), "setmotd", "t-"),
        "1The message of the day is the one in the world's settings again."
    );
    assert_eq!(run(&loader, &bob(), "motd", ""), "1Hi");
}

#[test]
fn an_announcement_reaches_every_player_at_once() {
    let dir = scratch("say");
    let loader = server("", &dir);
    assert_eq!(
        run(&loader, &ann(), "say", "tEvent at the keep in 5 min"),
        "12 players read it."
    );
    for key in [1, 2] {
        assert_eq!(said(&loader, key), ["Event at the keep in 5 min"]);
    }
    assert!(run(&loader, &bob(), "say", "tHello").starts_with('0'));
    assert_eq!(said(&loader, 1).len(), 1, "a refused line reaches nobody");
}

#[test]
fn a_player_goes_to_the_waypoint_the_caller_set_on_the_map() {
    let dir = scratch("mark");
    let eve = player(13, 4, "Eve", ADMIN);
    let loader = server_with("", &dir, &[eve]);
    assert_eq!(
        run(&loader, &ann(), "mark", "p0000000000000002"),
        "1Bob is on the way to 300, 25, -80."
    );
    let moved = loader.teleports();
    assert_eq!(moved.len(), 1);
    assert_eq!(moved[0].entity, Entity(11));
    assert_eq!(
        run(&loader, &eve, "mark", "p0000000000000002"),
        "0Set a waypoint on your map first."
    );
    assert_eq!(
        console::run("enshwick.admin-tools mark Bob").as_deref(),
        Ok("Run this as a player in the world: the console has no character.")
    );
    assert_eq!(loader.teleports().len(), 1);
}

/// A building event by Bob, who stands at 0, 20, 7.
fn building(kind: u8, position: [f32; 3]) -> BuildingData {
    BuildingData {
        header: Header::of::<BuildingData>(),
        entity: Entity(900),
        player: bob().entity,
        item: ItemId::new(0),
        kind,
        replay: 0,
        template: [0; 16],
        position,
    }
}

/// Bob digs out the voxel at `metres`.
fn dig(metres: [f64; 3]) -> [VoxelChange; 1] {
    let rock = Voxel::new(VoxelMaterial::Terrain(9), 255);
    let at = VoxelPos::from_metres(metres).unwrap();
    [VoxelChange::new(at, rock, Voxel::default())]
}

#[test]
fn the_build_log_stays_empty_and_the_lookup_says_so_unless_the_world_turns_it_on() {
    let dir = scratch("builds-off");
    let loader = server("", &dir);
    let placed = building(BuildingData::PLACED, [1.0, 20.0, 7.0]);
    assert!(loader.raise::<BuildingPlaced>(&placed) >= 1);
    let _handled = loader.voxels_changed(EditSource::Game, &dig([0.0, 20.0, 7.0]));
    assert_eq!(
        run(&loader, &ann(), "who", "n50"),
        "0The build log is off in this world's settings."
    );
    assert!(loader.raise::<ShutdownStarted>(&()) >= 1);
    assert!(!dir.join("builds.log").exists());
}

#[test]
fn the_build_log_names_who_placed_removed_and_dug_and_the_lookup_reads_the_newest_first() {
    let dir = scratch("builds");
    let loader = server("build_log = true\n", &dir);
    let placed = building(BuildingData::PLACED, [1.0, 20.0, 7.0]);
    assert!(loader.raise::<BuildingPlaced>(&placed) >= 1);
    let removed = building(BuildingData::TORN_DOWN, [0.0; 3]);
    assert!(loader.raise::<BuildingRemoved>(&removed) >= 1);
    for _ in 0..3 {
        // Strokes in one spot are one line.
        assert!(loader.voxels_changed(EditSource::Game, &dig([0.0, 20.0, 7.0])) >= 1);
    }
    // The mod's own writes and terrain far from every player name nobody.
    let _own = loader.voxels_changed(EditSource::ThisMod, &dig([0.0, 20.0, 7.0]));
    assert!(loader.voxels_changed(EditSource::Game, &dig([900.0, 20.0, 900.0])) >= 1);
    let log = file_until(&dir, "builds.log", |text| text.lines().count() == 4);
    assert_eq!(log.lines().count(), 4, "{log}");
    assert!(log.lines().last().unwrap().contains("\t-\tdug\t"), "{log}");

    let read = loader.type_in_chat(&ann(), 0, "/adm who 60").unwrap();
    let lines: Vec<&str> = read.lines().collect();
    assert_eq!(
        lines[0],
        "1Admin Tools: 3 changes within 60 m, newest first"
    );
    let by = "Bob (0000000000000002)";
    assert!(
        lines[1].ends_with(&format!("UTC: {by} dug terrain at 2, 22, 6")),
        "{read}"
    );
    assert!(
        lines[2].ends_with(&format!("UTC: {by} tore down a building at 0, 20, 7")),
        "where Bob stood: {read}"
    );
    assert!(
        lines[3].ends_with(&format!("UTC: {by} placed a building at 1, 20, 7")),
        "{read}"
    );
    assert_eq!(
        loader.type_in_chat(&ann(), 0, "/adm who 5").as_deref(),
        Some("1The build log has nothing within 5 m.")
    );
    assert!(run(&loader, &bob(), "who", "n60").starts_with('0'));
    assert_eq!(
        console::run("enshwick.admin-tools who 60").as_deref(),
        Ok("Run this as a player in the world: the console has no character.")
    );
}

#[test]
fn the_build_log_tells_a_building_picked_up_and_terrain_filled_from_the_rest() {
    let dir = scratch("builds-kinds");
    let loader = server("build_log = true\n", &dir);
    let picked = building(BuildingData::PICKED_UP, [0.0; 3]);
    assert!(loader.raise::<BuildingRemoved>(&picked) >= 1);
    let rock = Voxel::new(VoxelMaterial::Terrain(9), 255);
    let at = VoxelPos::from_metres([0.0, 20.0, 7.0]).unwrap();
    let fill = [VoxelChange::new(at, Voxel::default(), rock)];
    assert!(loader.voxels_changed(EditSource::Game, &fill) >= 1);
    let log = file_until(&dir, "builds.log", |text| text.lines().count() == 2);
    let kinds: Vec<&str> = log
        .lines()
        .map(|line| line.split('\t').nth(2).unwrap())
        .collect();
    assert_eq!(kinds, ["picked-up", "filled"], "{log}");
}
