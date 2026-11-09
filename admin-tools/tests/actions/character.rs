//! Actions on a player's character: health, stamina and mana set to a
//! value, and god mode until the player leaves.

use enshwick::Entity;
use enshwick::abi::{DAMAGE_TYPES, DamageCheck, Header, KnowledgeInfo, PlayerData};
use enshwick::events::PlayerLeft;
use enshwick::knowledge::KnowledgeId;

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

/// A player's knowledge `id` of `kind` at `value`, as Bob holds it.
fn known(id: u32, kind: u8, value: u32) -> (Option<Entity>, KnowledgeInfo) {
    let info = KnowledgeInfo {
        header: Header::of::<KnowledgeInfo>(),
        id: KnowledgeId::new(id),
        kind,
        scope: KnowledgeInfo::PLAYER,
        reserved: 0,
        value,
        flags: 0,
    };
    (Some(bob().entity), info)
}

#[test]
fn learn_unlocks_a_recipe_by_its_first_flag_and_sets_a_bool() {
    let dir = scratch("learn");
    let loader = server("", &dir);
    loader.set_knowledge(
        &[
            known(17, KnowledgeInfo::FLAGS, 0),
            known(18, KnowledgeInfo::BOOL, 0),
        ],
        &[],
    );
    assert_eq!(
        run(&loader, &ann(), "learn", "p0000000000000002\tt17"),
        "1Bob learns 17."
    );
    assert!(run(&loader, &ann(), "learn", "p0000000000000002\tt18").starts_with('1'));
    let grants: Vec<(u32, u32, u32)> = loader
        .grants()
        .into_iter()
        .map(|(_, id, kind, value)| (id.get(), kind, value))
        .collect();
    assert_eq!(
        grants,
        [
            (17, u32::from(KnowledgeInfo::FLAGS), 0),
            (18, u32::from(KnowledgeInfo::BOOL), 0)
        ]
    );
}

#[test]
fn learn_refuses_what_the_player_knows_a_count_and_an_unknown_id() {
    let dir = scratch("learn-refused");
    let loader = server("", &dir);
    loader.set_knowledge(
        &[
            known(17, KnowledgeInfo::FLAGS, 1),
            known(18, KnowledgeInfo::BOOL, 1),
            known(19, KnowledgeInfo::COUNTER, 0),
        ],
        &[],
    );
    assert_eq!(
        run(&loader, &ann(), "learn", "p0000000000000002\tt17"),
        "0Bob already knows 17."
    );
    assert_eq!(
        run(&loader, &ann(), "learn", "p0000000000000002\tt18"),
        "0Bob already knows 18."
    );
    assert_eq!(
        run(&loader, &ann(), "learn", "p0000000000000002\tt19"),
        "019 is a count, not something to unlock."
    );
    assert_eq!(
        run(&loader, &ann(), "learn", "p0000000000000002\tt20"),
        "0Admin Tools: nothing is named «20»."
    );
    assert!(loader.grants().is_empty());
}
