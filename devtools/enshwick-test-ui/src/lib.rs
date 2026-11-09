//! Development only, never shipped: the screen extensions, sounds and input
//! reads that the platform's interface checks are read against.
//!
//! On the server it marks a survivor, spawns test chests and refuses
//! placements near a Flame Altar; on a client it adds panels beside the
//! survivor dialog, the Flame Altar screen and a Safe's chest screen, and a
//! window on Shift+F10 that plays sounds, draws input hints and runs the
//! server's actions. While in a game it logs what the client reads of its
//! screen, prompt and input once a second, each time that changes. Each part
//! turns off alone, with its reason in the log.

mod screens;
mod server;
mod tone;
mod window;

use enshwick::actions::Actions;
use enshwick::i18n::Texts;
use enshwick::{Load, Mod, Off};

// The mod's name and description, in each language.
enshwick::texts!();

/// What the prompt of the survivor the mod marks says; the survivor
/// dialog's panel shows only after the player looked at it.
pub(crate) const SURVIVOR: &str = "Test survivor";

/// This machine's switches, `settings.toml` in the mod's own folder, read at
/// start.
#[enshwick::config]
#[derive(Debug, Default)]
pub(crate) struct Settings {
    /// The GUID of a creature's sound container, which the window's "Play
    /// creature sound" plays; empty for none.
    pub(crate) creature_sound: String,
    /// The GUID of a vanilla sound container to play at volume 0.2, such as
    /// a chest's opening sound; empty for none.
    pub(crate) quiet_sound: String,
}

/// The settings in `settings.toml` in the mod's folder; the defaults
/// without the file, or with a file that does not read.
fn own_settings(load: &Load<'_>) -> Settings {
    let Ok(path) = load.mod_dir().map(|dir| dir.join("settings.toml")) else {
        return Settings::default();
    };
    let Ok(text) = std::fs::read_to_string(&path) else {
        return Settings::default();
    };
    toml::from_str(&text).unwrap_or_else(|e| {
        enshwick::warn!("{}: {e}; the defaults apply", path.display());
        Settings::default()
    })
}

/// The test UI mod.
#[derive(Debug)]
pub struct TestUi;

impl Mod for TestUi {
    fn init(load: &mut Load<'_>) -> Result<Self, Off> {
        load.texts()?;
        let settings = own_settings(load);
        let actions: Actions = load.actions(Texts::default())?;
        server::register(load, &actions)?;
        let _refusal = load.feature("altar refusal", server::refuse_near_altars);
        if load.host().build().side == enshwick::abi::Side::CLIENT {
            screens::register(load, &actions);
            window::register(load, &actions, &settings);
        }
        enshwick::info!("test ui on");
        Ok(Self)
    }
}

enshwick::declare_mod!(TestUi);

#[cfg(test)]
mod tests {
    use std::time::Duration;

    use enshwick::abi::{Side, Status};
    use enshwick::test::{Drawn, Input};
    use enshwick::ui::HudScreenType;

    use super::*;

    fn start(side: Side, hosting: bool) -> (Status, enshwick::test::Installed) {
        let loader = enshwick::test::FakeHost::new()
            .developer_tools(true)
            .side(side)
            .hosting(hosting)
            .install();
        // SAFETY: the exports of `declare_mod!` above.
        let status =
            unsafe { loader.start(enshwick_mod_query, enshwick_mod_init, enshwick_mod_ready) };
        (status, loader)
    }

    #[test]
    fn the_test_ui_is_a_developer_tool() {
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
    fn every_text_exists_in_all_languages() {
        let texts = Texts::parse(texts::TOML).unwrap();
        assert_eq!(texts.untranslated(), []);
    }

    #[test]
    fn it_starts_on_every_side_without_a_panic() {
        for (side, hosting) in [
            (Side::SERVER, false),
            (Side::CLIENT, false),
            (Side::CLIENT, true),
        ] {
            let (status, loader) = start(side, hosting);
            assert_eq!(status, Status::OK, "{side:?}: {:?}", loader.faults());
            assert!(
                loader.logs().iter().any(|(_, line)| line == "test ui on"),
                "{side:?}: {:?}",
                loader.logs()
            );
            assert!(
                !loader.faults().iter().any(|fault| fault.contains("panic")),
                "{side:?}: {:?}",
                loader.faults()
            );
        }
    }

    fn buttons(drawn: &[(u32, Vec<Drawn>)]) -> Vec<String> {
        drawn
            .iter()
            .flat_map(|(_, widgets)| widgets)
            .filter_map(|widget| match widget {
                Drawn::Button { text, .. } => Some(text.clone()),
                _ => None,
            })
            .collect()
    }

    #[test]
    fn hello_shows_beside_the_dialog_only_after_the_marked_survivor_s_prompt() {
        let (_, loader) = start(Side::CLIENT, false);
        loader.show_prompt(Some(("Another survivor", 1)));
        let _drawn = loader.draw_frame();
        assert!(buttons(&loader.draw_screen(HudScreenType::NpcDialog)).is_empty());
        loader.show_prompt(Some((SURVIVOR, 1)));
        let _drawn = loader.draw_frame();
        // The dialog hides the prompt; the survivor stays marked.
        loader.show_prompt(None);
        let _drawn = loader.draw_frame();
        loader.input("Hello", Input::Click);
        assert_eq!(
            buttons(&loader.draw_screen(HudScreenType::NpcDialog)),
            ["Hello"]
        );
        assert_eq!(
            loader.action_runs(),
            [(server::HELLO.to_owned(), String::new())]
        );
    }

    #[test]
    fn a_safe_s_chest_screen_shows_safe_and_its_host_position() {
        let (_, loader) = start(Side::CLIENT, false);
        loader.host_screen(Some(enshwick::Entity(512)));
        loader.read_screen_host(screens::SAFE.guid(), [1.25, 2.5, -3.0]);
        let drawn = loader.draw_screen(HudScreenType::StorageChest);
        let labels: Vec<String> = drawn
            .iter()
            .flat_map(|(_, widgets)| widgets)
            .filter_map(|widget| match widget {
                Drawn::Label { text, .. } => Some(text.clone()),
                _ => None,
            })
            .collect();
        assert_eq!(labels, ["Safe", "1.2, 2.5, -3.0"]);
    }

    #[test]
    fn the_window_s_spawn_button_asks_the_server() {
        let (_, loader) = start(Side::CLIENT, false);
        assert!(!buttons(&[(0, loader.draw_frame())]).contains(&"Close".to_owned()));
        let key = enshwick::controls::Key::parse("Shift+F10").unwrap();
        assert_eq!(loader.keys(&[key], Duration::from_millis(16)), 1);
        loader.input("Spawn kept and unsaved", Input::Click);
        let drawn = buttons(&[(0, loader.draw_frame())]);
        assert!(drawn.contains(&"Close".to_owned()), "{drawn:?}");
        assert_eq!(
            loader.action_runs(),
            [(server::SPAWN.to_owned(), String::new())]
        );
    }
}
