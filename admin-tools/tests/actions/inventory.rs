//! Actions on what players carry: giving items by the game's name, GUID or
//! number, in as many stacks as they need, once; what did not fit; the
//! refusals, which the audit log keeps too.

use enshwick::ItemId;
use enshwick::abi::{
    InventoryOp, InventoryOpResult, InventoryRequest, InventoryResult, TransactionInfo,
};

use super::game::{
    BACKPACK, CHEST, GLIDER, LOOKS, WOOD, ann, bob, file_until, run, said, scratch, server,
    tick_until,
};

/// The gives the mod submitted so far: each one's inventory, item and count.
fn gives(loader: &enshwick::test::Installed) -> Vec<(u64, ItemId, u32)> {
    loader
        .inventory_requests()
        .iter()
        .flat_map(|request| request.ops.iter())
        .filter(|op| op.kind == InventoryOp::GIVE)
        .map(|op| (op.target, op.item, op.count))
        .collect()
}

/// Whether every open request commits what fits and leaves the rest out.
fn partial(loader: &enshwick::test::Installed) -> bool {
    loader
        .inventory_requests()
        .iter()
        .all(|request| request.flags & InventoryRequest::ALLOW_PARTIAL != 0)
}

/// An operation that ran and left `remaining` of its count out.
fn left(remaining: u32) -> InventoryOpResult {
    InventoryOpResult {
        error: 0,
        remaining,
    }
}

/// Whether Ann was told a line that starts with `start`.
fn ann_hears(loader: &enshwick::test::Installed, start: &str) -> bool {
    tick_until(loader, || {
        said(loader, 1).iter().any(|line| line.starts_with(start))
    })
}

/// Whether two positions are the same to a millimetre.
fn near(a: [f64; 3], b: [f64; 3]) -> bool {
    a.iter().zip(b).all(|(a, b)| (a - b).abs() < 1e-3)
}

#[test]
fn give_hands_a_player_an_item_named_as_the_game_names_it_once() {
    let dir = scratch("give");
    let loader = server("", &dir);
    let reply = run(
        &loader,
        &ann(),
        "give",
        "p0000000000000002\tttool_glider_t1\tn1",
    );
    assert_eq!(reply, "1Gave Tool_Glider_T1 × 1 to Bob.");
    let bob = u64::from(bob().entity.0);
    assert_eq!(gives(&loader), [(bob, ItemId::from_guid(GLIDER), 1)]);
}

#[test]
fn a_typed_give_without_a_count_hands_over_one_item() {
    let dir = scratch("give-one");
    let loader = server("", &dir);
    let typed = loader
        .type_in_chat(&ann(), 0, "/adm give Bob Tool_Glider_T1")
        .unwrap();
    assert_eq!(typed, "1Gave Tool_Glider_T1 × 1 to Bob.");
    let bob = u64::from(bob().entity.0);
    assert_eq!(gives(&loader), [(bob, ItemId::from_guid(GLIDER), 1)]);
}

#[test]
fn give_takes_an_item_s_guid_or_number_and_more_than_a_stack() {
    let dir = scratch("give-ids");
    let loader = server("", &dir);
    let (wood, glider) = (ItemId::from_guid(WOOD), ItemId::from_guid(GLIDER));
    let guid = format!("p0000000000000002\tt{WOOD}\tn100");
    assert!(run(&loader, &ann(), "give", &guid).starts_with('1'));
    let number = format!("p0000000000000002\tt{}\tn5", wood.get());
    assert!(run(&loader, &ann(), "give", &number).starts_with('1'));
    // A glider stacks to one; five of them take five slots.
    assert_eq!(
        run(
            &loader,
            &ann(),
            "give",
            "p0000000000000002\ttTool_Glider_T1\tn5"
        ),
        "1Gave Tool_Glider_T1 × 5 to Bob."
    );
    let bob = u64::from(bob().entity.0);
    assert_eq!(
        gives(&loader),
        [(bob, wood, 100), (bob, wood, 5), (bob, glider, 5)]
    );
    assert!(partial(&loader), "every give takes what fits");
}

#[test]
fn an_unknown_name_answers_with_the_names_near_it_and_goes_to_the_audit_log() {
    let dir = scratch("give-refused");
    let loader = server("audit = \"on\"\n", &dir);
    let reply = run(&loader, &ann(), "give", "p0000000000000002\ttGlider\tn1");
    assert!(
        reply.starts_with("0Admin Tools: nothing is named «Glider». Near: "),
        "one line in the interface: {reply}"
    );
    assert!(reply.contains("Tool_Glider_T1"), "{reply}");
    let typed = loader
        .type_in_chat(&ann(), 0, "/adm give Bob Glider")
        .unwrap();
    assert!(
        typed.starts_with("0Admin Tools: nothing is named «Glider».\nNear: "),
        "lines in chat: {typed}"
    );
    assert!(
        run(
            &loader,
            &bob(),
            "give",
            "p0000000000000002\ttTool_Glider_T1\tn1"
        )
        .starts_with('0')
    );
    assert!(gives(&loader).is_empty());
    let audit = file_until(&dir, "audit.log", |text| text.contains("Glider"));
    assert!(audit.contains("\tgive\tBob Glider 1\t"), "{audit}");
    assert!(
        audit.lines().all(|line| line.ends_with("\trefused")),
        "{audit}"
    );
}

#[test]
fn a_give_the_game_turns_down_is_told_to_the_administrator_and_kept_in_the_audit_log() {
    let dir = scratch("give-full");
    let loader = server("audit = \"on\"\n", &dir);
    assert!(
        run(
            &loader,
            &ann(),
            "give",
            "p0000000000000002\ttTool_Glider_T1\tn1"
        )
        .starts_with('1')
    );
    assert!(loader.finish_inventory(InventoryResult::ROLLED_BACK, &[]));
    assert!(ann_hears(
        &loader,
        "Bob got 0 of Tool_Glider_T1 × 1: 1 did not fit, or the game turned them down, and were left out."
    ));
    let audit = file_until(&dir, "audit.log", |text| text.contains("0 of"));
    let outcome = audit.lines().find(|line| line.contains("0 of")).unwrap();
    assert!(outcome.contains("\tinterface\tgive\t"), "{audit}");
    assert!(outcome.ends_with("\trefused"), "{audit}");
}

#[test]
fn a_count_with_a_fraction_is_refused_and_gives_nothing() {
    let dir = scratch("give-fraction");
    let loader = server("", &dir);
    assert_eq!(
        run(
            &loader,
            &ann(),
            "give",
            "p0000000000000002\ttTool_Glider_T1\tn1.5"
        ),
        "01.5 is not a whole number."
    );
    assert_eq!(
        run(
            &loader,
            &ann(),
            "chestat",
            "n10\tn20\tn30\ttResource_Wood\tn2.5"
        ),
        "02.5 is not a whole number."
    );
    assert!(loader.inventory_requests().is_empty());
    assert!(loader.transactions().is_empty());
}

#[test]
fn copy_gives_what_fits_leaves_the_look_slots_out_and_tells_how_many_stayed_out() {
    let dir = scratch("copy");
    let loader = server("audit = \"on\"\n", &dir);
    assert_eq!(
        run(
            &loader,
            &ann(),
            "copy",
            "p0000000000000001\tp0000000000000002"
        ),
        "1Copying what Ann carries to Bob."
    );
    let bob = u64::from(bob().entity.0);
    assert_eq!(
        gives(&loader),
        [
            (bob, ItemId::from_guid(WOOD), 30),
            (bob, ItemId::from_guid(GLIDER), 1)
        ]
    );
    assert!(partial(&loader), "what does not fit stays out");
    assert!(loader.finish_inventory(InventoryResult::COMMITTED, &[left(3), left(0)]));
    assert!(ann_hears(
        &loader,
        "3 items did not reach Bob: they did not fit, or the game turned them down, and were left out."
    ));
    let audit = file_until(&dir, "audit.log", |text| text.contains("did not reach"));
    assert!(
        audit.contains("\tcopy\tAnn Bob\t3 items did not reach Bob"),
        "{audit}"
    );
}

#[test]
fn a_copy_the_game_turns_down_whole_tells_every_item_stayed_out() {
    let dir = scratch("copy-refused");
    let loader = server("", &dir);
    assert!(
        run(
            &loader,
            &ann(),
            "copy",
            "p0000000000000001\tp0000000000000002"
        )
        .starts_with('1')
    );
    assert!(loader.finish_inventory(InventoryResult::ROLLED_BACK, &[]));
    assert!(ann_hears(&loader, "31 items did not reach Bob"));
}

#[test]
fn clear_destroys_every_stack_whole_and_a_part_the_game_turns_down_is_told_and_logged() {
    let dir = scratch("clear-filled");
    let loader = server("audit = \"on\"\n", &dir);
    assert_eq!(
        run(&loader, &ann(), "clear", "p0000000000000001"),
        "1Clearing what Ann carries."
    );
    let requests = loader.inventory_requests();
    let destroyed: Vec<u64> = requests
        .iter()
        .flat_map(|request| request.ops.iter())
        .filter(|op| op.kind == InventoryOp::DESTROY_SLOT)
        .map(|op| op.target)
        .collect();
    let slot = |inventory: u32, index: u64| u64::from(inventory) | index << 32;
    assert_eq!(
        destroyed,
        [slot(BACKPACK, 0), slot(BACKPACK, 1), slot(LOOKS, 0)],
        "looks included"
    );
    assert!(
        requests
            .iter()
            .all(|request| request.flags & InventoryRequest::ALLOW_PARTIAL == 0),
        "each part is whole or not at all"
    );
    assert!(loader.finish_inventory(InventoryResult::ROLLED_BACK, &[]));
    let turned_down = "The game turned down part of the change to what Ann carries.";
    assert!(ann_hears(&loader, turned_down));
    let audit = file_until(&dir, "audit.log", |text| text.contains(turned_down));
    let outcome = audit.lines().last().unwrap();
    assert!(outcome.contains("\tclear\tAnn\t"), "{audit}");
    assert!(outcome.ends_with("\trefused"), "{audit}");
}

#[test]
fn a_chest_the_game_never_places_is_undone_and_gets_no_items() {
    let dir = scratch("chest-pending");
    let loader = server("", &dir);
    assert!(
        run(
            &loader,
            &ann(),
            "chestat",
            "n10\tn20\tn30\ttResource_Wood\tn5"
        )
        .starts_with('1')
    );
    // No `run_transactions`: the placement stays queued for every look,
    // which the scheduler times by the clock: ten seconds.
    let lost = |loader: &enshwick::test::Installed| {
        said(loader, 1)
            .iter()
            .any(|line| line.starts_with("No chest went up at 10, 20, 30"))
    };
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(20);
    while !lost(&loader) && std::time::Instant::now() < deadline {
        let _ran = loader.tick(std::time::Duration::from_secs(1));
        std::thread::sleep(std::time::Duration::from_millis(50));
    }
    assert!(lost(&loader), "{:?}", said(&loader, 1));
    assert!(gives(&loader).is_empty());
    let state = loader.transactions()[0].info.state;
    assert!(
        [TransactionInfo::UNDOING, TransactionInfo::UNDONE].contains(&state),
        "no empty chest follows: {state}"
    );
}

#[test]
fn a_chest_whose_placement_fails_is_told_kept_in_the_audit_log_and_gets_no_items() {
    let dir = scratch("chest-failed");
    let loader = server("audit = \"on\"\n", &dir);
    assert!(
        run(
            &loader,
            &ann(),
            "chestat",
            "n10\tn20\tn30\ttResource_Wood\tn5"
        )
        .starts_with('1')
    );
    let placed = loader.transactions()[0].id;
    loader.fail_transaction(placed, enshwick::abi::Status::NOT_LOADED);
    assert!(ann_hears(&loader, "No chest went up at 10, 20, 30"));
    assert!(gives(&loader).is_empty());
    let audit = file_until(&dir, "audit.log", |text| text.contains("No chest"));
    let outcome = audit.lines().last().unwrap();
    assert!(
        outcome.contains("\tchestat\t10 20 30 Resource_Wood 5\t"),
        "{audit}"
    );
    assert!(outcome.ends_with("\trefused"), "{audit}");
}

#[test]
fn what_did_not_fit_is_told_with_how_many_arrived() {
    let dir = scratch("give-short");
    let loader = server("", &dir);
    assert!(
        run(
            &loader,
            &ann(),
            "give",
            "p0000000000000002\ttResource_Wood\tn250"
        )
        .starts_with('1')
    );
    let short = InventoryOpResult {
        error: 0,
        remaining: 50,
    };
    assert!(loader.finish_inventory(InventoryResult::COMMITTED, &[short]));
    assert!(ann_hears(
        &loader,
        "Bob got 200 of Resource_Wood × 250: 50 did not fit"
    ));
}

#[test]
fn clear_and_copy_refuse_an_empty_inventory_and_copy_refuses_one_player_twice() {
    let dir = scratch("clear");
    let loader = server("", &dir);
    assert_eq!(
        run(&loader, &ann(), "clear", "p0000000000000002"),
        "0Bob carries nothing."
    );
    assert_eq!(
        run(
            &loader,
            &ann(),
            "copy",
            "p0000000000000002\tp0000000000000001"
        ),
        "0Bob carries nothing."
    );
    assert_eq!(
        run(
            &loader,
            &ann(),
            "copy",
            "p0000000000000002\tp0000000000000002"
        ),
        "0Name two different players."
    );
    assert!(run(&loader, &bob(), "clear", "p0000000000000002").starts_with('0'));
    assert!(loader.inventory_requests().is_empty());
}

#[test]
fn a_chest_goes_up_at_a_position_and_then_gets_its_items() {
    let dir = scratch("chest-at");
    let loader = server("", &dir);
    assert_eq!(
        run(
            &loader,
            &ann(),
            "chestat",
            "n10\tn20\tn30\ttResource_Wood\tn250"
        ),
        "1A chest with Resource_Wood × 250 goes up at 10, 20, 30."
    );
    let chest = ItemId::from_guid(CHEST);
    assert!(tick_until(&loader, || loader.props().iter().any(
        |(_, item, at)| *item == chest && near(*at, [10.0, 20.0, 30.0])
    )));
    let (entity, _, _) = loader.props()[0];
    assert!(tick_until(&loader, || gives(&loader)
        == [(u64::from(entity.0), ItemId::from_guid(WOOD), 250)]));
}

#[test]
fn a_chest_at_a_player_stands_beside_them_and_bob_may_not_place_one() {
    let dir = scratch("chest");
    let loader = server("", &dir);
    assert_eq!(
        run(
            &loader,
            &ann(),
            "chest",
            "p0000000000000002\ttTool_Glider_T1\tn1"
        ),
        "1A chest with Tool_Glider_T1 × 1 goes up at Bob."
    );
    assert!(tick_until(&loader, || loader
        .props()
        .iter()
        .any(|(_, _, at)| near(*at, [2.0, 20.0, 7.0]))));
    assert!(run(&loader, &bob(), "chestat", "n0\tn0\tn0\ttResource_Wood\tn1").starts_with('0'));
}
