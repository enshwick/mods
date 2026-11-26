use enshwick::abi::{PlayerInfo, Side, Status, UiMarker};
use enshwick::config::Config as _;
use enshwick::content::EngineFacts as _;
use enshwick::i18n::{Language, Texts};
use enshwick::test::FakeHost;
use enshwick::test::facts::{self, Fact};
use enshwick::{Entity, PlayerKey};

use super::*;

const NAME: &str = "marks@1";

fn start(host: FakeHost) -> enshwick::test::Installed {
    let loader = host.install();
    // SAFETY: the exports of `declare_mod!` in this crate.
    let status = unsafe { loader.start(enshwick_mod_query, enshwick_mod_init, enshwick_mod_ready) };
    assert_eq!(status, Status::OK, "{:?}", loader.faults());
    let _handlers = loader.raise::<enshwick::events::SettingsReplaced>(&());
    loader
}

fn player(entity: u32, key: u64) -> PlayerInfo {
    enshwick::abi::headed!(PlayerInfo {
        slot: entity,
        entity: Entity::from_engine(entity),
        key: PlayerKey::new(key),
        permissions: 0,
        name_len: 0,
        name: [0; PlayerInfo::NAME_MAX],
    })
}

/// A current transform at `x` metres along, laid out as `host` places it.
fn at(host: &FakeHost, x: f64) -> Vec<u8> {
    let offset = |fact: Fact| host.offset(fact.id()).unwrap();
    let place =
        offset(facts::CURRENT_TRANSFORM_TRANSFORM) + offset(facts::WORLD_TRANSFORM_POSITION);
    let mut bytes = vec![0u8; place + 24];
    for (axis, value) in enshwick::ecs::fixed([x, 0.0, 0.0]).iter().enumerate() {
        bytes[place + 8 * axis..][..8].copy_from_slice(&value.to_le_bytes());
    }
    bytes
}

/// A server with a player at the origin whose client runs the mod, a second
/// one whose client does not, and the containers `holders` gives: a
/// marker fact and how far along each is.
fn server(holders: &[(Fact, f64)]) -> FakeHost {
    let host = FakeHost::new().world().inventory().container_kinds();
    let hash = |fact: Fact| host.hash(fact.id()).unwrap();
    let (setup, transform) = (hash(facts::INVENTORY_SETUP), hash(facts::CURRENT_TRANSFORM));
    let markers: Vec<(u32, f64)> = holders
        .iter()
        .map(|&(marker, x)| (hash(marker), x))
        .collect();
    let here = at(&host, 0.0);
    let mut host = host
        .player(player(10, 1))
        .player(player(11, 2))
        .component(Entity::from_engine(10), transform, &here)
        .component(Entity::from_engine(11), transform, &here)
        .net_peer(Peer::Player(PlayerKey::new(1)));
    for (i, (marker, x)) in markers.into_iter().enumerate() {
        let entity = Entity::from_engine(100 + u32::try_from(i).unwrap());
        host = host
            .placed(setup, entity, [x, 0.0, 0.0])
            .component(entity, marker, &[]);
    }
    host
}

/// The marks the server sent, each with the key of the player it went to.
fn sent(loader: &enshwick::test::Installed) -> Vec<(PlayerKey, Vec<Mark>)> {
    loader
        .net_sent()
        .into_iter()
        .filter(|(name, _, _)| name == NAME)
        .map(|(_, key, bytes)| (key, postcard::from_bytes(&bytes).unwrap()))
        .collect()
}

#[test]
fn the_default_settings_parse_and_out_of_range_radii_are_refused() {
    let settings: Settings = toml::from_str(&Settings::template()).unwrap();
    assert!(settings.chests && settings.resources);
    assert!((settings.radius - 150.0).abs() < f64::EPSILON);
    assert!(settings.validate().is_ok());
    let with = |radius| Settings {
        radius,
        ..Settings::default()
    };
    assert!(with(9.9).validate().is_err());
    assert!(with(1000.1).validate().is_err());
    assert!(with(f64::NAN).validate().is_err());
    assert!(with(1000.0).validate().is_ok());
}

#[test]
fn every_text_the_mod_shows_exists_in_every_required_language() {
    let texts = Texts::parse(texts::TOML).unwrap();
    assert_eq!(texts.untranslated(), []);
    let schema = Settings::schema();
    let keys = std::iter::once(schema.text)
        .chain(
            schema
                .fields
                .into_iter()
                .flat_map(|field| [format!("{}.doc", field.text), field.text]),
        )
        .chain(
            [
                enshwick::i18n::MOD_NAME,
                enshwick::i18n::MOD_DESCRIPTION,
                CHEST_LABEL,
                RESOURCE_LABEL,
            ]
            .map(str::to_owned),
        );
    for key in keys {
        for &language in Language::REQUIRED {
            assert!(
                texts.get(&key, language).is_some(),
                "{}: {key}",
                language.code()
            );
        }
    }
}

#[test]
fn the_server_sends_each_player_running_the_mod_the_chests_and_resources_within_the_radius() {
    let loader = start(
        server(&[
            (facts::LOOT_FILL, 10.0),
            (facts::RESOURCE_NODE, 20.0),
            (facts::LOOT_FILL, 500.0),
        ])
        .settings("chests = true\nresources = true\nradius = 150.0\n"),
    );
    assert_eq!(loader.tick(EVERY), 1);
    assert_eq!(
        sent(&loader),
        [(
            PlayerKey::new(1),
            vec![
                Mark {
                    kind: Kind::Chest,
                    at: [10.0, 0.0, 0.0]
                },
                Mark {
                    kind: Kind::Resource,
                    at: [20.0, 0.0, 0.0]
                },
            ]
        )]
    );
}

#[test]
fn a_kind_turned_off_in_the_settings_is_not_sent() {
    let loader = start(
        server(&[(facts::LOOT_FILL, 10.0), (facts::RESOURCE_NODE, 20.0)])
            .settings("chests = false\nresources = true\nradius = 150.0\n"),
    );
    assert_eq!(loader.tick(EVERY), 1);
    let marks = sent(&loader);
    assert_eq!(marks.len(), 1, "{marks:?}");
    assert!(
        marks[0].1.iter().all(|mark| mark.kind == Kind::Resource),
        "{marks:?}"
    );
}

#[test]
fn a_full_set_of_marks_leaves_in_one_message() {
    let holders: Vec<(Fact, f64)> = (0..MAX_MARKERS + 50)
        .map(|i| {
            (
                facts::RESOURCE_NODE,
                f64::from(u32::try_from(i).unwrap()) * 0.5,
            )
        })
        .collect();
    let loader =
        start(server(&holders).settings("chests = true\nresources = true\nradius = 1000.0\n"));
    assert_eq!(loader.tick(EVERY), 1);
    let marks = sent(&loader);
    assert_eq!(marks.len(), 1);
    assert_eq!(marks[0].1.len(), MAX_MARKERS);
}

#[test]
fn the_client_shows_the_server_s_marks_with_their_colour_and_label() {
    let loader = start(
        FakeHost::new()
            .side(Side::CLIENT)
            .world()
            .net_peer(Peer::Server),
    );
    let marks = vec![
        Mark {
            kind: Kind::Chest,
            at: [1.0, 2.0, 3.0],
        },
        Mark {
            kind: Kind::Resource,
            at: [4.0, 5.0, 6.0],
        },
    ];
    assert!(loader.net_deliver(NAME, Peer::Server, &marks));
    let shown: Vec<(u32, String, [f64; 3])> = loader
        .markers()
        .into_iter()
        .map(|marker| (marker.color, marker.label, marker.position))
        .collect();
    assert_eq!(
        shown,
        [
            (UiMarker::YELLOW, "Chest".to_owned(), [1.0, 2.0, 3.0]),
            (UiMarker::GREEN, "Resource".to_owned(), [4.0, 5.0, 6.0]),
        ]
    );
}

#[test]
fn the_client_ignores_marks_that_do_not_come_from_the_server_and_bounds_what_it_shows() {
    let loader = start(
        FakeHost::new()
            .side(Side::CLIENT)
            .world()
            .net_peer(Peer::Server),
    );
    let mark = Mark {
        kind: Kind::Chest,
        at: [1.0, 2.0, 3.0],
    };
    assert!(loader.net_deliver(NAME, Peer::Player(PlayerKey::new(5)), &vec![mark]));
    assert!(loader.markers().is_empty());
    let many: Vec<Mark> = (0..MAX_MARKERS + 10)
        .map(|i| Mark {
            kind: Kind::Chest,
            at: [f32::from(u16::try_from(i).unwrap()), 0.0, 0.0],
        })
        .collect();
    assert!(loader.net_deliver(NAME, Peer::Server, &many));
    assert_eq!(loader.markers().len(), MAX_MARKERS);
}

#[test]
fn a_server_with_no_player_running_the_mod_sends_nothing() {
    let host = FakeHost::new().world().inventory().container_kinds();
    let hash = |fact: Fact| host.hash(fact.id()).unwrap();
    let (setup, loot) = (hash(facts::INVENTORY_SETUP), hash(facts::LOOT_FILL));
    let loader = start(
        host.player(player(10, 1))
            .placed(setup, Entity::from_engine(100), [1.0, 0.0, 0.0])
            .component(Entity::from_engine(100), loot, &[]),
    );
    assert_eq!(loader.tick(EVERY), 1);
    assert!(sent(&loader).is_empty());
}

fn chest_at(x: f32) -> Mark {
    Mark {
        kind: Kind::Chest,
        at: [x, 0.0, 0.0],
    }
}

#[test]
fn the_client_takes_its_marks_off_the_map_once_the_server_is_gone() {
    let loader = start(FakeHost::new().side(Side::CLIENT).world());
    assert!(loader.net_deliver(NAME, Peer::Server, &vec![chest_at(1.0)]));
    assert_eq!(loader.markers().len(), 1);
    let _drawn = loader.draw_frame();
    assert!(loader.markers().is_empty());
    assert!(loader.net_deliver(NAME, Peer::Server, &vec![chest_at(1.0)]));
    assert_eq!(loader.markers().len(), 1, "the same set shows again");
}

#[test]
fn the_client_keeps_its_marks_while_the_server_is_there() {
    let loader = start(
        FakeHost::new()
            .side(Side::CLIENT)
            .world()
            .net_peer(Peer::Server),
    );
    assert!(loader.net_deliver(NAME, Peer::Server, &vec![chest_at(1.0)]));
    let _drawn = loader.draw_frame();
    assert_eq!(loader.markers().len(), 1);
}

#[test]
fn the_same_marks_are_labelled_again_in_a_new_language() {
    let loader = start(
        FakeHost::new()
            .side(Side::CLIENT)
            .world()
            .net_peer(Peer::Server),
    );
    let shown = SHOWN.get().unwrap();
    show(shown, vec![chest_at(1.0)], Language::EN_US);
    show(shown, vec![chest_at(1.0)], Language::UK_UA);
    let labels: Vec<String> = loader
        .markers()
        .into_iter()
        .map(|marker| marker.label)
        .collect();
    assert_eq!(labels, ["Скриня"]);
}

#[test]
fn a_dedicated_server_runs_the_mod_without_the_map() {
    let loader = start(server(&[]));
    assert_eq!(loader.tick(EVERY), 1);
    assert!(loader.markers().is_empty());
}
