//! Actions on a player's character: health, stamina and mana set to a
//! value, and god mode until the player leaves.

use enshwick::Entity;
use enshwick::abi::{DAMAGE_TYPES, DamageCheck, Header, PlayerData};
use enshwick::events::PlayerLeft;

use super::game::{HEALTH, ann, bob, run, scratch, server};

/// A fall that takes `total` health from `target`.
fn fall(target: Entity, total: f32) -> DamageCheck {
    DamageCheck {
        header: Header::of::<DamageCheck>(),
        cause: DamageCheck::FALL_DAMAGE,
        target,
        source: Entity(0),
        attacker: Entity(0),
        amounts: [0.0; DAMAGE_TYPES],
        total,
    }
}

#[test]
fn stats_sets_a_player_s_health_and_says_what_the_game_kept() {
    let dir = scratch("stats");
    let loader = server("", &dir);
    assert_eq!(
        run(&loader, &ann(), "stats", "p0000000000000002\tc0\tn120"),
        "1Bob: Health 120 of 200."
    );
    let bytes = loader.component_bytes(bob().entity, HEALTH).unwrap();
    assert_eq!(i32::from_le_bytes(bytes[8..12].try_into().unwrap()), 120);
    assert!(run(&loader, &bob(), "stats", "p0000000000000002\tc0\tn200").starts_with('0'));
}

#[test]
fn stats_refuses_a_pool_the_character_lacks() {
    let dir = scratch("stats-lacks");
    let loader = server("", &dir);
    let reply = run(&loader, &ann(), "stats", "p0000000000000002\tc1\tn50");
    assert!(reply.starts_with("0Bob: nothing changed"), "{reply}");
}

#[test]
fn god_mode_cancels_damage_to_its_player_only_until_they_leave() {
    let dir = scratch("god");
    let loader = server("", &dir);
    let (ann, bob) = (ann(), bob());
    assert_eq!(
        run(&loader, &ann, "god", "p0000000000000002\tb1"),
        "1Bob is in god mode: nothing damages them."
    );
    assert_eq!(loader.damage(fall(bob.entity, 40.0)).total, 0.0);
    assert_eq!(loader.damage(fall(ann.entity, 40.0)).total, 40.0);
    let left = PlayerData {
        header: Header::of::<PlayerData>(),
        slot: bob.slot,
        entity: bob.entity,
        key: bob.key,
    };
    assert!(loader.raise::<PlayerLeft>(&left) >= 1);
    assert_eq!(loader.damage(fall(bob.entity, 40.0)).total, 40.0);
}

#[test]
fn god_mode_turns_off_on_request() {
    let dir = scratch("god-off");
    let loader = server("", &dir);
    assert!(run(&loader, &ann(), "god", "p0000000000000002\tb1").starts_with('1'));
    assert_eq!(
        run(&loader, &ann(), "god", "p0000000000000002\tb0"),
        "1Bob takes damage again."
    );
    assert_eq!(loader.damage(fall(bob().entity, 40.0)).total, 40.0);
}
