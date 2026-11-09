//! Actions on what players carry: giving items by the game's name, GUID or
//! number, within a stack, once, and the refusals.

use enshwick::ItemId;
use enshwick::abi::{InventoryOp, InventoryResult};

use super::game::{CHEST, GLIDER, WOOD, ann, bob, run, said, scratch, server, tick_until};

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
        "p0000000000000002\tn1\tttool_glider_t1",
    );
    assert_eq!(reply, "11 × tool_glider_t1 on the way to Bob.");
    let bob = u64::from(bob().entity.0);
    assert_eq!(gives(&loader), [(bob, ItemId::from_guid(GLIDER), 1)]);
}

#[test]
fn give_takes_an_item_s_guid_or_number_and_counts_up_to_a_stack() {
    let dir = scratch("give-ids");
    let loader = server("", &dir);
    let wood = ItemId::from_guid(WOOD);
    let guid = format!("p0000000000000002\tn100\tt{WOOD}");
    assert!(run(&loader, &ann(), "give", &guid).starts_with('1'));
    let number = format!("p0000000000000002\tn5\tt{}", wood.get());
    assert!(run(&loader, &ann(), "give", &number).starts_with('1'));
    let bob = u64::from(bob().entity.0);
    assert_eq!(gives(&loader), [(bob, wood, 100), (bob, wood, 5)]);
}

#[test]
fn give_refuses_more_than_a_stack_and_an_unknown_name_with_the_names_near_it() {
    let dir = scratch("give-refused");
    let loader = server("", &dir);
    assert_eq!(
        run(
            &loader,
            &ann(),
            "give",
            "p0000000000000002\tn2\ttTool_Glider_T1"
        ),
        "0Tool_Glider_T1 stacks up to 1; give at most that many at once."
    );
    let reply = run(
        &loader,
        &ann(),
        "give",
        "p0000000000000002\tn1\ttTool_Glider",
    );
    assert!(
        reply.starts_with("0Nothing is named Tool_Glider. Near: "),
        "{reply}"
    );
    assert!(reply.contains("Tool_Glider_T1"), "{reply}");
    assert!(
        run(
            &loader,
            &bob(),
            "give",
            "p0000000000000002\tn1\ttTool_Glider_T1"
        )
        .starts_with('0')
    );
    assert!(gives(&loader).is_empty());
}

#[test]
fn a_give_the_game_turns_down_is_told_to_the_administrator() {
    let dir = scratch("give-full");
    let loader = server("", &dir);
    assert!(
        run(
            &loader,
            &ann(),
            "give",
            "p0000000000000002\tn1\ttTool_Glider_T1"
        )
        .starts_with('1')
    );
    assert!(loader.finish_inventory(InventoryResult::ROLLED_BACK, &[]));
    assert!(tick_until(&loader, || said(&loader, 1).iter().any(
        |line| line.starts_with("Bob did not get 1 × Tool_Glider_T1")
    )));
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
            "n10\tn20\tn30\tn250\ttResource_Wood"
        ),
        "1A chest with 250 × Resource_Wood goes up at 10, 20, 30."
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
            "p0000000000000002\tn1\ttTool_Glider_T1"
        ),
        "1A chest with 1 × Tool_Glider_T1 goes up at Bob."
    );
    assert!(tick_until(&loader, || loader
        .props()
        .iter()
        .any(|(_, _, at)| near(*at, [2.0, 20.0, 7.0]))));
    assert!(run(&loader, &bob(), "chestat", "n0\tn0\tn0\tn1\ttResource_Wood").starts_with('0'));
}
