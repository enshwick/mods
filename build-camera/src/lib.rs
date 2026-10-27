//! Airborne Architect's building fly mode for every player.
//!
//! The game opens fly mode near a building zone to a character that may edit
//! the base, is alive, not gliding, and unlocked the Airborne Architect node.
//! This mod answers "unlocked" at the fly mode gate alone, so skill points,
//! prerequisites and the skill tree stay vanilla. `enabled` is a join value: a
//! joined player predicts with the server's.

use core::sync::atomic::{AtomicBool, Ordering};

use enshwick::progress::{Skill, SkillGate};
use enshwick::state::ModLocal;
use enshwick::{Load, Mod, Off};

/// The labels and descriptions of the settings, a table per language.
const TEXTS: &str = include_str!("texts.toml");

/// Whether fly mode is open without the skill, for this mod life.
static ENABLED: ModLocal<AtomicBool> = ModLocal::new();

/// Whether fly mode is open without the skill; off until the settings are
/// read.
fn enabled() -> &'static AtomicBool {
    ENABLED.get_or_init(|| AtomicBool::new(false))
}

/// Build camera settings.
#[derive(Debug, serde::Deserialize, serde::Serialize, enshwick::Config)]
#[serde(deny_unknown_fields)]
struct Settings {
    /// Every player may use the building fly mode near a base without the
    /// Airborne Architect skill. The vanilla limits stay: near a building
    /// zone, allowed to edit the base, alive and not gliding.
    /// A joined player runs with the server's value.
    #[config(sync)]
    enabled: bool,
}

impl Default for Settings {
    fn default() -> Self {
        Self { enabled: true }
    }
}

/// The answer to a check of the Airborne Architect node.
///
/// # Examples
///
/// ```
/// use enshwick::progress::SkillGate;
///
/// assert_eq!(enshwick_build_camera::lifts(SkillGate::FlyMode, true), Some(true));
/// assert_eq!(enshwick_build_camera::lifts(SkillGate::FlyMode, false), None);
/// assert_eq!(enshwick_build_camera::lifts(SkillGate::Other, true), None);
/// ```
#[must_use]
pub fn lifts(gate: SkillGate, enabled: bool) -> Option<bool> {
    (enabled && gate == SkillGate::FlyMode).then_some(true)
}

/// The mod.
#[derive(Debug)]
pub struct BuildCamera;

fn apply(settings: &Settings) {
    let was = enabled().swap(settings.enabled, Ordering::Relaxed);
    if was != settings.enabled {
        enshwick::info!(
            "fly mode without the skill: {}",
            if settings.enabled { "on" } else { "off" }
        );
    }
}

impl Mod for BuildCamera {
    fn manifest() -> &'static enshwick::abi::Manifest {
        enshwick::manifest!(
            id: "enshwick.build-camera",
            side: BOTH,
            name: "Build camera",
            join: true,
            was: ["build-camera"],
        )
    }

    fn init(load: &mut Load<'_>) -> Result<Self, Off> {
        let _texts = load.texts(TEXTS)?;
        let settings: Settings = load.config()?;
        load.on_skill(Skill::AirborneArchitect, |check| {
            lifts(check.gate, enabled().load(Ordering::Relaxed))
        })?;
        load.on_config_change(|settings: Settings| apply(&settings))?;
        apply(&settings);
        Ok(Self)
    }
}

enshwick::declare_mod!(BuildCamera);

#[cfg(test)]
mod tests {
    use enshwick::abi::{Header, SkillCheck as RawCheck, Status};
    use enshwick::config::Config as _;
    use enshwick::events;

    use super::*;

    /// The node the fake host names for Airborne Architect.
    const NODE: u32 = 0x51;

    #[test]
    fn enabled_is_the_join_value_and_on_by_default() {
        let settings: Settings = toml::from_str(&Settings::template()).unwrap();
        assert!(settings.enabled);
        let synced = settings.synced().unwrap();
        assert_eq!(synced, "{ enabled = true }");
        let mut local: Settings = toml::from_str("enabled = true").unwrap();
        local.merge_synced("{ enabled = false }").unwrap();
        assert!(!local.enabled);
    }

    #[test]
    fn every_text_exists_in_all_languages() {
        let texts = enshwick::i18n::Texts::parse(TEXTS).unwrap();
        assert_eq!(texts.untranslated(), []);
    }

    #[test]
    fn the_mod_and_every_setting_have_their_texts() {
        let texts = enshwick::i18n::Texts::parse(TEXTS).unwrap();
        let schema = Settings::schema();
        let keys = std::iter::once(schema.text)
            .chain(
                schema
                    .fields
                    .into_iter()
                    .flat_map(|field| [format!("{}.doc", field.text), field.text]),
            )
            .chain([enshwick::i18n::MOD_NAME, enshwick::i18n::MOD_DESCRIPTION].map(str::to_owned));
        for key in keys {
            let found = texts.get(&key, enshwick::i18n::Language::FALLBACK);
            assert!(found.is_some(), "{key}");
        }
    }

    #[test]
    fn the_gate_follows_the_settings_and_the_servers_join_value() {
        let loader = enshwick::test::FakeHost::new()
            .settings("enabled = true\n")
            .constant("skill.airborne_architect", NODE.into())
            .install();
        // SAFETY: the exports of `declare_mod!` above.
        let status = unsafe { loader.start(enshwick_mod_init, enshwick_mod_ready) };
        assert_eq!(status, Status::OK);
        let ask = |node, gate| {
            loader.skill(&RawCheck {
                header: Header::of::<RawCheck>(),
                node,
                gate,
                unlocked: 0,
                reserved: [0; 3],
            })
        };

        assert_eq!(ask(NODE, RawCheck::FLY_MODE), RawCheck::UNLOCKED);
        assert_eq!(
            ask(NODE, RawCheck::OTHER),
            RawCheck::VANILLA,
            "another caller stays vanilla"
        );
        assert_eq!(
            ask(NODE + 1, RawCheck::FLY_MODE),
            RawCheck::VANILLA,
            "another node stays vanilla"
        );

        loader.set_server_synced(Some("{ enabled = false }"));
        assert_eq!(loader.raise::<events::SettingsChanged>(&()), 1);
        assert_eq!(
            ask(NODE, RawCheck::FLY_MODE),
            RawCheck::VANILLA,
            "the server turned it off"
        );

        loader.set_server_synced(None);
        assert_eq!(loader.raise::<events::SettingsChanged>(&()), 1);
        assert_eq!(
            ask(NODE, RawCheck::FLY_MODE),
            RawCheck::UNLOCKED,
            "the file's value again"
        );
        assert!(loader.faults().is_empty());
    }
}
