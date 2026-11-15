//! Spawns and props: creatures named by template spawned around a player or
//! at a position as the game's admin spawn does, and props placed and taken
//! back.

use enshwick::ItemId;
use enshwick::abi::SpawnRequest;
use enshwick::test::CREATURE;

use super::game::{CHEST, ann, bob, run, scratch, server, tick_until};

/// The spawns asked for so far.
fn spawns(loader: &enshwick::test::Installed) -> Vec<SpawnRequest> {
    loader
        .spawns()
        .into_iter()
        .map(|(_, request)| request)
        .collect()
}

#[test]
fn spawn_puts_each_creature_on_a_ring_around_the_player_out_of_the_save() {
    let dir = scratch("spawn");
    let loader = server("", &dir);
    assert_eq!(
        run(
            &loader,
            &ann(),
            "spawn",
            &format!("p0000000000000002\te{CREATURE}\tn3\tn0\tn0\tb0")
        ),
        format!("1{CREATURE} × 3 appear at Bob.")
    );
    let asked = spawns(&loader);
    assert_eq!(asked.len(), 3);
    for request in &asked {
        assert_eq!(request.template, CREATURE);
        assert_eq!(request.flags, SpawnRequest::DO_NOT_SAVE);
        assert_eq!((request.level, request.lifetime_ns), (0, 0));
        let [x, y, z] = request.position;
        let away = (x - 0.0).hypot(z - 7.0);
        assert!(
            (away - 3.0).abs() < 1e-6 && (y - 20.0).abs() < 1e-6,
            "{x} {y} {z}"
        );
    }
}

#[test]
fn a_typed_spawn_takes_a_level_a_lifetime_and_a_keep_or_leaves_them_out() {
    let dir = scratch("spawn-typed");
    let loader = server("", &dir);
    assert_eq!(
        enshwick::console::run(&format!("adm spawnat 1 2 3 {CREATURE}")),
        Ok(format!("{CREATURE} × 1 appear at 1, 2, 3."))
    );
    let [one] = spawns(&loader)[..] else {
        panic!("one spawn");
    };
    assert_eq!(one.position, [1.0, 2.0, 3.0], "one stands on the spot");
    assert!(
        enshwick::console::run(&format!("adm spawnat 1 2 3 {CREATURE} 1 12 5 on"))
            .unwrap()
            .starts_with(&format!("{CREATURE} × 1"))
    );
    let kept = spawns(&loader)[1];
    assert_eq!(kept.level, 12);
    assert_eq!(kept.lifetime_ns, 5 * 60 * 1_000_000_000);
    assert_eq!(kept.flags, 0, "kept in the save");
}

#[test]
fn a_template_the_game_does_not_know_spawns_nothing() {
    let dir = scratch("spawn-unknown");
    let loader = server("", &dir);
    let unknown = enshwick::content::Guid([0x77; 16]);
    let line = format!("n0\tn0\tn0\te{unknown}\tn1\tn0\tn0\tb0");
    let reply = run(&loader, &ann(), "spawnat", &line);
    assert_eq!(reply, format!("0No template {unknown}."));
    assert!(spawns(&loader).is_empty());
}

#[test]
fn the_spawns_window_offers_the_template_picker_for_spawns_and_the_item_picker_for_props() {
    use enshwick::actions::sheet::ArgView;

    let dir = scratch("spawn-picker");
    let loader = server("", &dir);
    assert_eq!(run(&loader, &ann(), "open", "c5"), "1");
    let drawn = super::windows::window(&loader, &ann(), "spawns").entries;
    let pickers = [
        ("spawn", ArgView::TEMPLATE),
        ("spawnat", ArgView::TEMPLATE),
        ("prop", ArgView::ITEM),
        ("propat", ArgView::ITEM),
    ];
    for (id, kind) in pickers {
        let entry = drawn
            .iter()
            .find(|entry| entry.id == id)
            .unwrap_or_else(|| panic!("no {id} in the spawns window"));
        let picked = entry.args.iter().filter(|arg| arg.kind == kind);
        assert_eq!(picked.count(), 1, "{id}");
    }
}

#[test]
fn a_template_s_guid_spawns_it_and_bob_may_not_spawn() {
    let dir = scratch("spawn-guid");
    let loader = server("", &dir);
    let line = format!("n5\tn6\tn7\te{CREATURE}\tn1\tn0\tn0\tb0");
    assert!(run(&loader, &ann(), "spawnat", &line).starts_with('1'));
    assert_eq!(spawns(&loader)[0].template, CREATURE);
    assert!(run(&loader, &bob(), "spawnat", &line).starts_with('0'));
    assert_eq!(spawns(&loader).len(), 1);
}

#[test]
fn a_prop_goes_up_beside_the_player_and_the_last_one_is_taken_back_once() {
    let dir = scratch("prop");
    let loader = server("", &dir);
    assert_eq!(
        enshwick::console::run("adm unprop").as_deref(),
        Ok("You placed no prop to take back.")
    );
    let reply = run(
        &loader,
        &ann(),
        "prop",
        &format!("p0000000000000002\ti{CHEST}"),
    );
    assert_eq!(reply, "1Prop_Storage_32_T2_Chest_Wood goes up at Bob.");
    let chest = ItemId::from_guid(CHEST);
    assert!(tick_until(&loader, || loader.props().iter().any(
        |(_, item, at)| *item == chest
            && at
                .iter()
                .zip([2.0, 20.0, 7.0])
                .all(|(a, b)| (a - b).abs() < 1e-3)
    )));
    // The mod remembers the prop at a look after the game placed it; a
    // take-back before that finds none and changes nothing.
    assert!(tick_until(&loader, || run(&loader, &ann(), "unprop", "")
        == "1Your last prop, Prop_Storage_32_T2_Chest_Wood, will be taken back."));
    assert!(tick_until(&loader, || loader.props().is_empty()));
    assert_eq!(
        run(&loader, &ann(), "unprop", ""),
        "0You placed no prop to take back."
    );
}
