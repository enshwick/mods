//! Development only, never shipped: content of its own for the in-game
//! checks of the platform's content API, on the server and on clients.
//!
//! It adds a copy of the game's rain to the first weather layer, which holds
//! no rain of its own, picked there with probability 1 and wide enough to
//! reach the starting area wherever it starts; an enemy copied from a wolf,
//! sent to the first player who joins; a Flame Altar kind with a wider zone,
//! held at level 1; and a chest of its own. Each joining player gets the
//! altar and the chest, and learns the chest's recipe. Each part turns off
//! alone, with its reason on the Mods screen.

mod join;

use core::time::Duration;

use enshwick::altars::{KindZone, NewFlameAltar};
use enshwick::content::enemies::NewEnemy;
use enshwick::content::items::NewItem;
use enshwick::content::props::{NewProp, PropId};
use enshwick::content::templates::TemplateId;
use enshwick::content::{Guid, Id, id, vanilla};
use enshwick::resources::Registry;
use enshwick::typed::{ResourceType, WeatherPhenomenonEffect as GameEffect, WeatherSystemResource};
use enshwick::weather::{NewPhenomenon, PhenomenonChange, WeatherPhenomenonEffect};
use enshwick::{Load, Mod, Off};

// The mod's name and description, in each language.
enshwick::texts!();

/// The rain's content id.
const RAIN: &str = "enshwick.test-content:rain";
/// How far each front of it reaches, in metres: wider than the map's
/// starting area from anywhere a front may start.
const RAIN_RADIUS: f32 = 15_000.0;
/// How long each front of it lasts, in game time.
const RAIN_LASTS: (Duration, Duration) = (Duration::from_mins(5), Duration::from_mins(10));

/// A rain phenomenon of the game's and the first weather layer, by GUID,
/// read from the game's weather resource.
fn vanilla_rain(load: &Load<'_>) -> Result<(Guid, Guid), Off> {
    let ty = ResourceType::<WeatherSystemResource>::resolve(load)?;
    let hash = ty.key(Guid([0; 16])).type_hash;
    let host = enshwick::host().ok_or_else(|| Off::new("no loader"))?;
    let key = host
        .resource_keys(Registry::game()?.hashed(hash))?
        .into_iter()
        .find(|key| key.type_hash == hash)
        .ok_or_else(|| Off::new("the game has no weather resource"))?;
    let system = ty.game_value(key)?;
    let layer = system
        .layers
        .first()
        .ok_or_else(|| Off::new("the weather resource has no layer"))?
        .guid;
    let rain = system
        .phenomenons
        .iter()
        .find(|phenomenon| phenomenon.effect == GameEffect::Rain)
        .ok_or_else(|| Off::new("the weather resource has no rain"))?
        .guid;
    Ok((rain, layer))
}

/// Adds the rain.
fn rain(load: &mut Load<'_>) -> Result<(), Off> {
    let (like, layer) = vanilla_rain(load)?;
    let values = PhenomenonChange::new()
        .probability(1.0)
        .radius(RAIN_RADIUS, RAIN_RADIUS)
        .duration(RAIN_LASTS.0, RAIN_LASTS.1)
        .effect(WeatherPhenomenonEffect::Rain);
    let new = NewPhenomenon::new(Id::new(RAIN), like)
        .in_layer(layer)
        .values(values);
    let added = load.add_weather_phenomenon(new)?;
    enshwick::info!(
        "rain {} added to the first weather layer {layer}, a copy of {like}",
        added.0
    );
    Ok(())
}

/// The vanilla wolf the enemy copies, as the template collection of server
/// 26430253 names it.
const WOLF: &str = "Enemy_Wildbeast_wolf_earlygame";
/// The enemy.
const ENEMY: Id = id!("wolf");
/// The enemy's level and its health against the vanilla wolf's.
const ENEMY_LEVEL: u32 = 30;
const ENEMY_HEALTH: f32 = 2.0;

/// The vanilla Flame Altar item the altar kind copies, as the item registry
/// of server 26430253 and client 26430625 lists it.
const VANILLA_ALTAR: &str = "Prop_Building_T0_FlameAltar_Small_Build_Base";
/// The altar kind's item and its name text.
const ALTAR: Id = id!("altar");
const ALTAR_NAME: Id = ALTAR.part("name");
/// The kind's zone at level 1: 40 cells more on each side of the vanilla
/// zone, 80 cells across at level 1 on server 26083076, so twice as wide;
/// the vanilla height.
const ALTAR_ZONE: KindZone = KindZone {
    grow: 40,
    height: 80,
};
/// The kind's highest level.
const ALTAR_MAX_LEVEL: u8 = 1;

/// The vanilla wooden chest the chest copies, as the item registry of server
/// 399 and client 426 lists it.
const VANILLA_CHEST: &str = "Prop_Storage_32_T2_Chest_Wood";
/// The chest's item and its name text.
const CHEST: Id = id!("chest");
const CHEST_NAME: Id = CHEST.part("name");
/// Its recipe's ingredients against the vanilla chest's.
const CHEST_COST: u32 = 1;

/// The vanilla item or template named `name`, or why not.
fn named(name: &str, find: fn(&str) -> Result<Guid, vanilla::VanillaError>) -> Result<Guid, Off> {
    find(name).map_err(|why| Off::new(format!("{name}: {why}")))
}

/// Adds the enemy.
fn enemy(load: &mut Load<'_>) -> Result<TemplateId, Off> {
    let like = named(WOLF, vanilla::template)?;
    let added = load.add_enemy(NewEnemy {
        level: Some(ENEMY_LEVEL),
        health: Some(ENEMY_HEALTH),
        ..NewEnemy::new(ENEMY, like)
    })?;
    enshwick::info!(
        "enemy {} added, a copy of {WOLF} {like} at level {ENEMY_LEVEL} with {ENEMY_HEALTH} times its health",
        added.guid()
    );
    Ok(added)
}

/// Adds the Flame Altar kind and holds it at its highest level.
fn altar(load: &mut Load<'_>) -> Result<PropId, Off> {
    let like = named(VANILLA_ALTAR, vanilla::item)?;
    let kind = load.add_flame_altar(NewFlameAltar {
        item: NewItem {
            name: Some(ALTAR_NAME.guid()),
            ..NewItem::new(ALTAR, like)
        },
        zones: vec![ALTAR_ZONE],
        recipe_cost: None,
    })?;
    load.limit_altar_level(kind, ALTAR_MAX_LEVEL)?;
    enshwick::info!(
        "altar kind {} added, a copy of {VANILLA_ALTAR} {like} placing {}, zone {ALTAR_ZONE:?}, at most level {ALTAR_MAX_LEVEL}",
        kind.item.0,
        kind.template.guid()
    );
    Ok(kind)
}

/// Adds the chest and its recipe.
fn chest(load: &mut Load<'_>) -> Result<PropId, Off> {
    let like = named(VANILLA_CHEST, vanilla::item)?;
    let chest = load.add_prop(NewProp {
        recipe_cost: Some(CHEST_COST),
        ..NewProp::new(NewItem {
            name: Some(CHEST_NAME.guid()),
            ..NewItem::new(CHEST, like)
        })
    })?;
    enshwick::info!(
        "chest {} added, a copy of {VANILLA_CHEST} {like} placing {}, with its recipe",
        chest.item.0,
        chest.template.guid()
    );
    Ok(chest)
}

/// The test content.
#[derive(Debug)]
pub struct TestContent;

impl Mod for TestContent {
    fn init(load: &mut Load<'_>) -> Result<Self, Off> {
        let texts = load.texts()?;
        load.add_texts(
            &texts,
            &[
                (texts::ALTAR_NAME, ALTAR_NAME),
                (texts::CHEST_NAME, CHEST_NAME),
            ],
        )?;
        load.feature("rain", rain);
        let enemy = load.feature("enemy", enemy);
        let altar = load.feature("altar kind", altar);
        let chest = load.feature("chest", chest);
        join::link(load, join::Gifts::new(enemy, altar, chest));
        Ok(Self)
    }
}

enshwick::declare_mod!(TestContent);

#[cfg(test)]
mod tests {
    use enshwick::abi::Status;
    use enshwick::i18n::Texts;

    use super::*;

    #[test]
    fn every_text_exists_in_all_languages() {
        let texts = Texts::parse(texts::TOML).unwrap();
        assert_eq!(texts.untranslated(), []);
    }

    #[test]
    fn the_test_content_is_a_developer_tool() {
        // SAFETY: `enshwick_mod_query` returns the `'static` manifest of
        // `declare_mod!`, which no thread writes.
        let manifest = unsafe { &*enshwick_mod_query() };
        assert!(
            manifest
                .flags
                .contains(enshwick::abi::ManifestFlags::DEVELOPER)
        );
    }

    #[test]
    fn without_the_game_s_data_each_part_turns_off_alone_with_its_reason() {
        let loader = enshwick::test::FakeHost::new()
            .developer_tools(true)
            .install();
        // SAFETY: the exports of `declare_mod!` above.
        let status =
            unsafe { loader.start(enshwick_mod_query, enshwick_mod_init, enshwick_mod_ready) };
        assert_eq!(status, Status::OK, "{:?}", loader.faults());
        let features = loader.features();
        for part in ["rain", "enemy", "altar kind", "chest"] {
            let reason = features
                .iter()
                .find(|(name, _)| name == part)
                .and_then(|(_, reason)| reason.as_deref());
            assert!(
                reason.is_some_and(|reason| !reason.is_empty() && !reason.contains("panic")),
                "{part}: {features:?}"
            );
        }
    }
}
