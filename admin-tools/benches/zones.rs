//! The build filter of protected zones, which runs on every frame a player
//! aims to build: one check through the fake loader with no zone, and with
//! the most zones a world keeps, refused in the first, allowed after all of
//! them, and allowed inside one for a zone builder named by account.

use std::hint::black_box;

use criterion::{Criterion, criterion_group, criterion_main};
use enshwick::abi::BuildCheck;
use enshwick::{Entity, ItemId};

#[path = "../tests/actions/game.rs"]
#[expect(
    dead_code,
    unused_imports,
    clippy::unwrap_used,
    reason = "the tests' fake game, written for tests: the bench uses only its start and run"
)]
mod game;

/// Ann's character in the tests' fake game.
const ANN: Entity = Entity::from_raw(10).unwrap();
/// Bob's character in the tests' fake game.
const BOB: Entity = Entity::from_raw(11).unwrap();

/// Bob's placement at `at` metres, as the cursor check hands it over.
fn check(at: [f64; 3]) -> BuildCheck {
    enshwick::abi::headed!(BuildCheck {
        character: BOB,
        item: ItemId::new(7),
        position: at,
        reason: core::ptr::null_mut(),
        account: enshwick::PlayerKey::new(2),
    })
}

fn filter(c: &mut Criterion) {
    let dir = game::scratch("bench-zones");
    let loader = game::server("", &dir);
    let open = check([500.0, 12.0, 500.0]);
    c.bench_function("build check, no zone", |b| {
        b.iter(|| loader.build(black_box(&open)));
    });
    // Each box centres on Ann at 40, 12, -3; the open spot lies outside all 64.
    for n in 0..64 {
        let line = format!("tz{n}\tn10\tn10\tn10");
        assert!(game::run(&loader, &game::ann(), "zonebox", &line).starts_with('1'));
    }
    let inside = check([40.0, 12.0, -3.0]);
    assert!(loader.build(&inside).is_err());
    c.bench_function("build check, 64 zones, refused", |b| {
        b.iter(|| loader.build(black_box(&inside)));
    });
    assert!(loader.build(&open).is_ok());
    c.bench_function("build check, 64 zones, allowed", |b| {
        b.iter(|| loader.build(black_box(&open)));
    });
    let builder = enshwick::abi::headed!(BuildCheck {
        character: ANN,
        account: enshwick::PlayerKey::new(1),
        ..inside
    });
    assert!(loader.build(&builder).is_ok());
    c.bench_function("build check, 64 zones, zone builder by account", |b| {
        b.iter(|| loader.build(black_box(&builder)));
    });
}

criterion_group!(benches, filter);
criterion_main!(benches);
