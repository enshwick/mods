//! Protected zones: the build filter refuses a placement inside an active
//! zone for every player the world's rule leaves out, the zones outlive a
//! restart, and only administrators change them.

use enshwick::abi::{BuildCheck, Header};
use enshwick::events::SettingsReplaced;
use enshwick::{Entity, ItemId};

use super::game::{ann, bob, file_until, run, scratch, server, tick_until};

/// The reason key the filter hands the builder.
const REFUSED: &str = "notice.zone";

/// Whether the character `player` may place something at `at` metres, or
/// the reason it may not.
fn build(
    loader: &enshwick::test::Installed,
    player: u32,
    at: [f64; 3],
) -> Result<(), Option<String>> {
    loader.build(&BuildCheck {
        header: Header::of::<BuildCheck>(),
        player: Entity(player),
        item: ItemId::new(7),
        position: enshwick::ecs::fixed(at),
        reason: core::ptr::null_mut(),
    })
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
        "1Only zone builders may build within 10 m of 40, -3, in the zone market."
    );
    assert_eq!(
        build(&loader, BOB, [42.0, 500.0, -3.0]),
        Err(Some(REFUSED.to_owned())),
        "every height of a circle"
    );
    assert_eq!(build(&loader, BOB, [55.0, 12.0, -3.0]), Ok(()));
    assert_eq!(build(&loader, ANN, [42.0, 12.0, -3.0]), Ok(()));
}

#[test]
fn a_box_refuses_only_what_lies_inside_it_at_its_height() {
    let dir = scratch("zone-box");
    let loader = server("", &dir);
    assert_eq!(
        run(&loader, &ann(), "zonebox", "thall\tn10\tn4\tn6"),
        "1Only zone builders may build in the zone hall: 10 × 4 × 6 m around 40, 12, -3."
    );
    assert!(build(&loader, BOB, [44.0, 13.0, -1.0]).is_err());
    assert_eq!(build(&loader, BOB, [44.0, 15.0, -1.0]), Ok(()), "above it");
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
        "1The zone market is off: everyone may build in it."
    );
    assert_eq!(build(&loader, BOB, [40.0, 12.0, -3.0]), Ok(()));
    assert!(run(&loader, &ann(), "zoneswitch", "tmarket\tb1").starts_with('1'));
    assert!(build(&loader, BOB, [40.0, 12.0, -3.0]).is_err());
    assert_eq!(
        run(&loader, &ann(), "unzone", "tmarket"),
        "1The zone market is removed."
    );
    assert_eq!(build(&loader, BOB, [40.0, 12.0, -3.0]), Ok(()));
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
            "market (on): 10 m around 40, -3\n",
            "hall (off): 10 × 4 × 6 m around 40, 12, -3"
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
    assert!(build(&loader, BOB, [40.0, 12.0, -3.0]).is_err());
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
    assert!(tick_until(&loader, || build(
        &loader,
        BOB,
        [40.0, 12.0, -3.0]
    )
    .is_ok()));
    loader.set_settings("settings", "zone_builders = \"nobody\"\n");
    assert!(loader.raise::<SettingsReplaced>(&()) >= 1);
    assert!(tick_until(&loader, || build(
        &loader,
        ANN,
        [40.0, 12.0, -3.0]
    )
    .is_err()));
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
