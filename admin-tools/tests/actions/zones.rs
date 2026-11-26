//! Protected zones: the build filter refuses a placement inside an active
//! zone for every player the world's rule leaves out, the zones outlive a
//! restart, and only administrators change them.

use enshwick::abi::{BuildCheck, PlayerData, PlayerInfo};
use enshwick::events::{PlayerJoined, SettingsReplaced, ShutdownStarted};
use enshwick::{Entity, ItemId, PlayerKey};

use super::game::{DRIVE, ann, bob, file_until, hosted, run, said, scratch, server};

/// The reason key the filter hands the builder.
const REFUSED: &str = "notice.zone";

/// Whether the character `player` may place something at `at` metres, or
/// the reason it may not.
fn build(
    loader: &enshwick::test::Installed,
    player: u32,
    at: [f64; 3],
) -> Result<(), Option<String>> {
    loader.build(&enshwick::abi::headed!(BuildCheck {
        character: Entity::from_engine(player),
        item: ItemId::new(7),
        position: at,
        reason: core::ptr::null_mut(),
        account: PlayerKey::default(),
    }))
}

/// Ann's and Bob's characters.
const ANN: u32 = 10;
const BOB: u32 = 11;

#[test]
fn a_zone_refuses_bob_inside_it_and_lets_him_build_outside_while_ann_builds_anywhere() {
    let dir = scratch("zone");
    let loader = server("", &dir);
    assert_eq!(
        run(&loader, &ann(), "zone", "tmarket\tn10"),
        "1Only zone builders may build, dig or break within 10 m of 40, 3, in the zone market."
    );
    assert_eq!(
        build(&loader, BOB, [42.0, 500.0, 3.0]),
        Err(Some(REFUSED.to_owned())),
        "every height of a circle"
    );
    assert_eq!(build(&loader, BOB, [55.0, 12.0, 3.0]), Ok(()));
    assert_eq!(build(&loader, ANN, [42.0, 12.0, 3.0]), Ok(()));
}

#[test]
fn the_host_s_own_prediction_lets_her_build_in_a_zone_with_a_character_no_server_player_names() {
    let dir = scratch("zone-hosted");
    let loader = hosted("", &dir);
    assert!(run(&loader, &ann(), "zone", "tmarket\tn10").starts_with('1'));
    assert_eq!(build(&loader, 40, [42.0, 12.0, 3.0]), Ok(()));
    assert!(build(&loader, BOB, [42.0, 12.0, 3.0]).is_err());
}

#[test]
fn a_box_refuses_only_what_lies_inside_it_at_its_height() {
    let dir = scratch("zone-box");
    let loader = server("", &dir);
    assert_eq!(
        run(&loader, &ann(), "zonebox", "thall\tn10\tn4\tn6"),
        "1Only zone builders may build, dig or break in the zone hall: 10 × 4 × 6 m around 40, 12, 3."
    );
    assert!(build(&loader, BOB, [44.0, 13.0, 5.0]).is_err());
    assert_eq!(build(&loader, BOB, [44.0, 15.0, 5.0]), Ok(()), "above it");
}

#[test]
fn a_zone_switched_off_or_removed_lets_everyone_build_and_a_name_is_taken_once() {
    let dir = scratch("zone-switch");
    let loader = server("", &dir);
    assert!(run(&loader, &ann(), "zone", "tmarket\tn10").starts_with('1'));
    assert_eq!(
        run(&loader, &ann(), "zone", "tMarket\tn5"),
        "0A zone is named Market already."
    );
    assert_eq!(
        run(&loader, &ann(), "zoneswitch", "tMARKET\tb0"),
        "1The zone market is off: it stops nothing until it is switched on."
    );
    assert_eq!(build(&loader, BOB, [40.0, 12.0, 3.0]), Ok(()));
    assert!(run(&loader, &ann(), "zoneswitch", "tmarket\tb1").starts_with('1'));
    assert!(build(&loader, BOB, [40.0, 12.0, 3.0]).is_err());
    assert_eq!(
        run(&loader, &ann(), "unzone", "tmarket"),
        "1The zone market is removed."
    );
    assert_eq!(build(&loader, BOB, [40.0, 12.0, 3.0]), Ok(()));
    assert_eq!(
        run(&loader, &ann(), "unzone", "tmarket"),
        "0No zone is named market."
    );
}

#[test]
fn the_zones_list_each_zone_with_its_state_and_place() {
    let dir = scratch("zone-list");
    let loader = server("", &dir);
    assert_eq!(
        enshwick::console::run("adm zones").as_deref(),
        Ok("No zone protects this world.")
    );
    run(&loader, &ann(), "zone", "tmarket\tn10");
    run(&loader, &ann(), "zonebox", "thall\tn10\tn4\tn6");
    run(&loader, &ann(), "zoneswitch", "thall\tb0");
    assert_eq!(
        enshwick::console::run("adm zones").as_deref(),
        Ok(concat!(
            "Admin Tools: 2 zones\n",
            "market (on): 10 m around 40, 3; stops building, breaking\n",
            "hall (off): 10 × 4 × 6 m around 40, 12, 3; stops building, breaking"
        ))
    );
}

#[test]
fn zones_are_written_at_once_and_guard_the_world_after_a_restart() {
    let dir = scratch("zone-kept");
    {
        let loader = server("", &dir);
        assert!(run(&loader, &ann(), "zone", "tmarket\tn10").starts_with('1'));
        let kept = file_until(&dir, "zones.toml", |text| text.contains("market"));
        assert!(kept.contains("kind = \"circle\""), "{kept}");
    }
    let loader = server("", &dir);
    assert!(build(&loader, BOB, [40.0, 12.0, 3.0]).is_err());
}

#[test]
fn zones_that_do_not_read_hold_building_everywhere_for_all_but_zone_builders() {
    let dir = scratch("zone-unread");
    std::fs::create_dir_all(&dir).unwrap();
    std::fs::write(dir.join("zones.toml"), "zones = 7\n").unwrap();
    let loader = server("", &dir);
    assert_eq!(
        build(&loader, BOB, [900.0, 0.0, -900.0]),
        Err(Some(REFUSED.to_owned()))
    );
    assert_eq!(build(&loader, ANN, [900.0, 0.0, -900.0]), Ok(()));
}

#[test]
fn the_zone_builders_rule_lets_others_build_and_bob_may_not_make_a_zone() {
    let dir = scratch("zone-builders");
    let loader = server("", &dir);
    run(&loader, &ann(), "zone", "tmarket\tn10");
    assert!(run(&loader, &bob(), "zone", "tmine\tn10").starts_with('0'));
    loader.set_settings("settings", "zone_builders = \"everyone\"\n");
    assert!(loader.raise::<SettingsReplaced>(&()) >= 1);
    assert!(loader.drive_until(DRIVE, || build(&loader, BOB, [40.0, 12.0, 3.0]).is_ok()));
    loader.set_settings("settings", "zone_builders = \"nobody\"\n");
    assert!(loader.raise::<SettingsReplaced>(&()) >= 1);
    assert!(loader.drive_until(DRIVE, || build(&loader, ANN, [40.0, 12.0, 3.0]).is_err()));
}

#[test]
fn the_console_has_nowhere_to_make_a_zone() {
    let dir = scratch("zone-console");
    let _loader = server("", &dir);
    assert_eq!(
        enshwick::console::run("adm zone market 10").as_deref(),
        Ok("Run this as a player in the world: the console has no character.")
    );
}

/// The bits a zone that stops breaking takes.
const BREAKING: u32 =
    PlayerInfo::CAN_EDIT_WORLD | PlayerInfo::CAN_EDIT_BASE | PlayerInfo::CAN_EXTEND_BASE;

/// Bob's account.
const BOB_KEY: PlayerKey = PlayerKey::new(2);

#[test]
fn a_zone_takes_digging_from_bob_near_it_tells_him_once_and_gives_it_back_when_it_goes_off() {
    let dir = scratch("zone-limits");
    let loader = server("", &dir);
    // Bob stands about 41 m from Ann, inside a circle of 50 m around her.
    assert!(run(&loader, &ann(), "zone", "tmarket\tn50").starts_with('1'));
    assert!(loader.drive_until(DRIVE, || loader.permission_change(BOB_KEY) == (BREAKING, 0)));
    assert_eq!(
        loader.permission_change(ann().key),
        (0, 0),
        "a zone builder"
    );
    let notice = "You are at a protected zone: digging, breaking and building are off here.";
    assert_eq!(
        said(&loader, 2)
            .iter()
            .filter(|line| *line == notice)
            .count(),
        1,
        "{:?}",
        said(&loader, 2)
    );
    assert!(run(&loader, &ann(), "zoneswitch", "tmarket\tb0").starts_with('1'));
    assert!(loader.drive_until(DRIVE, || loader.permission_change(BOB_KEY) == (0, 0)));
}

#[test]
fn the_margin_reaches_past_the_zone_s_edge_and_a_margin_of_0_does_not() {
    let dir = scratch("zone-margin");
    let loader = server("", &dir);
    // 35 m plus the 10 m margin reaches Bob at about 41 m.
    assert!(run(&loader, &ann(), "zone", "tmarket\tn35").starts_with('1'));
    assert!(loader.drive_until(DRIVE, || loader.permission_change(BOB_KEY) == (BREAKING, 0)));
    loader.set_settings("settings", "zone_margin = 0\n");
    assert!(loader.raise::<SettingsReplaced>(&()) >= 1);
    assert!(loader.drive_until(DRIVE, || loader.permission_change(BOB_KEY) == (0, 0)));
}

#[test]
fn a_zone_set_to_stop_only_chests_lets_bob_build_and_takes_only_chests() {
    let dir = scratch("zone-rules");
    let loader = server("", &dir);
    assert!(run(&loader, &ann(), "zone", "tmarket\tn50").starts_with('1'));
    assert_eq!(
        run(&loader, &ann(), "zonerules", "tMARKET\tb0\tb0\tb1"),
        "1The zone market stops building: no; digging and breaking: no; opening chests: yes."
    );
    assert!(
        loader.drive_until(DRIVE, || loader.permission_change(BOB_KEY)
            == (PlayerInfo::CAN_ACCESS_INVENTORIES, 0))
    );
    assert_eq!(build(&loader, BOB, [42.0, 12.0, 3.0]), Ok(()));
    assert!(run(&loader, &bob(), "zonerules", "tmarket\tb1\tb1\tb1").starts_with('0'));
}

#[test]
fn a_respawned_zone_builder_builds_at_once_and_a_respawned_bob_is_refused() {
    let dir = scratch("zone-respawn");
    let loader = server("", &dir);
    assert!(run(&loader, &ann(), "zone", "tmarket\tn10").starts_with('1'));
    assert!(loader.respawn(ann().key, Entity::from_engine(40)));
    assert!(loader.respawn(BOB_KEY, Entity::from_engine(41)));
    assert_eq!(build(&loader, 40, [42.0, 12.0, 3.0]), Ok(()));
    assert_eq!(
        build(&loader, 41, [42.0, 12.0, 3.0]),
        Err(Some(REFUSED.to_owned()))
    );
}

#[test]
fn after_the_world_stops_the_zones_take_no_rights_again() {
    let dir = scratch("zone-stopped");
    let loader = server("", &dir);
    let notice = "You are at a protected zone: digging, breaking and building are off here.";
    let notices = || {
        said(&loader, 2)
            .iter()
            .filter(|line| *line == notice)
            .count()
    };
    assert!(run(&loader, &ann(), "zone", "tmarket\tn50").starts_with('1'));
    assert!(loader.drive_until(DRIVE, || notices() == 1));
    assert!(loader.raise::<ShutdownStarted>(&()) >= 1);
    for _ in 0..3 {
        let _ran = loader.tick(std::time::Duration::from_secs(1));
    }
    assert_eq!(notices(), 1, "{:?}", said(&loader, 2));
}

#[test]
fn removing_the_zone_gives_bob_his_rights_back() {
    let dir = scratch("zone-removed");
    let loader = server("", &dir);
    assert!(run(&loader, &ann(), "zone", "tmarket\tn50").starts_with('1'));
    assert!(loader.drive_until(DRIVE, || loader.permission_change(BOB_KEY) == (BREAKING, 0)));
    assert!(run(&loader, &ann(), "unzone", "tmarket").starts_with('1'));
    assert!(loader.drive_until(DRIVE, || loader.permission_change(BOB_KEY) == (0, 0)));
}

#[test]
fn a_rejoin_inside_the_zone_takes_the_rights_again_and_a_character_with_no_place_keeps_them_off() {
    let dir = scratch("zone-rejoin");
    let loader = server("", &dir);
    let notice = "You are at a protected zone: digging, breaking and building are off here.";
    let notices = || {
        said(&loader, 2)
            .iter()
            .filter(|line| *line == notice)
            .count()
    };
    assert!(run(&loader, &ann(), "zone", "tmarket\tn50").starts_with('1'));
    assert!(loader.drive_until(DRIVE, || notices() == 1));
    // The platform ends a change as its player leaves, so the mod takes the
    // rights again for the player who joins back in the zone.
    let joined = enshwick::abi::headed!(PlayerData {
        slot: BOB,
        entity: Entity::from_engine(BOB),
        key: BOB_KEY,
    });
    assert!(loader.raise::<PlayerJoined>(&joined) >= 1);
    assert!(loader.drive_until(DRIVE, || notices() == 2));
    assert_eq!(loader.permission_change(BOB_KEY), (BREAKING, 0));
    // A new character with no place yet, as after a respawn at an altar:
    // what the zones took stays until the place reads.
    assert!(loader.respawn(BOB_KEY, Entity::from_engine(41)));
    for _ in 0..3 {
        let _ran = loader.tick(std::time::Duration::from_secs(1));
    }
    assert_eq!(loader.permission_change(BOB_KEY), (BREAKING, 0));
}
