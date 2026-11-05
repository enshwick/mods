//! Development only, never shipped: content of its own for the in-game
//! checks of the platform's content API, on the server and on clients.
//!
//! It adds a copy of the game's rain to the first weather layer, which holds
//! no rain of its own, picked there with probability 1 and wide enough to
//! reach the starting area wherever it starts.

use core::time::Duration;

use enshwick::content::{Guid, Id};
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

/// The test content.
#[derive(Debug)]
pub struct TestContent;

impl Mod for TestContent {
    fn init(load: &mut Load<'_>) -> Result<Self, Off> {
        load.texts()?;
        rain(load)?;
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
    fn without_the_game_s_weather_it_turns_off_with_the_reason() {
        let loader = enshwick::test::FakeHost::new()
            .developer_tools(true)
            .install();
        // SAFETY: the exports of `declare_mod!` above.
        let status =
            unsafe { loader.start(enshwick_mod_query, enshwick_mod_init, enshwick_mod_ready) };
        assert_eq!(status, Status::REFUSED);
        let faults = loader.faults();
        assert_eq!(faults.len(), 1, "{faults:?}");
        assert!(!faults[0].contains("panic"), "{faults:?}");
    }
}
