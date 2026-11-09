//! Development only, never shipped: the screen extensions, sounds and input
//! reads that the platform's interface checks are read against.
//!
//! On the server it marks a survivor, spawns test chests and refuses
//! placements near a Flame Altar; on a client it adds panels beside the
//! survivor dialog, the Flame Altar screen and a storage chest's screen, and a
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
    /// The GUID of the template whose storage chests get the "Chest" panel,
    /// as the read line names it while such a chest's screen is open; empty
    /// for none.
    pub(crate) chest_template: String,
}

/// The settings in `settings.toml` in the mod's folder; the defaults
/// without the file, or with a file that does not read.
fn own_settings(load: &Load<'_>) -> Settings {
    let Ok(path) = load.mod_dir().map(|dir| dir.join("settings.toml")) else {
        return Settings::default();
    };
    let text = match std::fs::read_to_string(&path) {
        Ok(text) => text,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Settings::default(),
        Err(e) => {
            enshwick::warn!("{}: {e}; the defaults apply", path.display());
            return Settings::default();
        }
    };
    toml::from_str(&text).unwrap_or_else(|e| {
        enshwick::warn!("{}: {e}; the defaults apply", path.display());
        Settings::default()
    })
}

/// The GUID the setting `name` holds, or `None` with the reason in the log.
pub(crate) fn guid(setting: &str, name: &str) -> Option<enshwick::content::Guid> {
    if setting.is_empty() {
        enshwick::info!("no {name} in settings.toml");
        return None;
    }
    let guid = enshwick::content::Guid::try_parse(setting);
    if guid.is_none() {
        enshwick::warn!("{name} {setting} is no GUID");
    }
    guid
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
            screens::register(load, &actions, &settings);
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

    /// The mod's own folder in this test: one per test thread.
    fn own_dir() -> std::path::PathBuf {
        std::env::temp_dir().join(format!(
            "enshwick-test-ui-{}-{:?}",
            std::process::id(),
            std::thread::current().id()
        ))
    }

    /// The mod started on `host` with `settings` as the file in its own
    /// folder, which the tone is written into.
    fn start_on(
        host: enshwick::test::FakeHost,
        settings: &[u8],
    ) -> (Status, enshwick::test::Installed) {
        let dir = own_dir();
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(dir.join("settings.toml"), settings).unwrap();
        let loader = host
            .developer_tools(true)
            .mod_dir(dir.to_str().unwrap())
            .install();
        // SAFETY: the exports of `declare_mod!` above.
        let status =
            unsafe { loader.start(enshwick_mod_query, enshwick_mod_init, enshwick_mod_ready) };
        (status, loader)
    }

    fn on(side: Side) -> enshwick::test::FakeHost {
        enshwick::test::FakeHost::new().side(side)
    }

    fn start(side: Side, hosting: bool) -> (Status, enshwick::test::Installed) {
        start_on(on(side).hosting(hosting), b"")
    }

    fn logged(loader: &enshwick::test::Installed, part: &str) -> bool {
        loader.logs().iter().any(|(_, line)| line.contains(part))
    }

    #[test]
    fn the_tone_is_written_and_the_sound_the_settings_name_is_quieted() {
        use enshwick::resources::{Field, Kind, Type, fnv1a32};

        let container = "demo::SoundContainerResource";
        let host = on(Side::CLIENT)
            .types([
                Type::new("demo::float32", Kind::Float32, 4, 4),
                Type::new(container, Kind::Struct, 4, 4).with_fields(vec![Field::new(
                    "volume",
                    0,
                    "demo::float32",
                )]),
            ])
            .type_hash(
                enshwick::content::types::SOUND_CONTAINER_RESOURCE.id(),
                fnv1a32(container.as_bytes()),
            );
        let sound = enshwick::content::Guid::named("test:sound");
        let settings = format!("creature_sound = \"{sound}\"\nquiet_sound = \"{sound}\"\n");
        let (status, loader) = start_on(host, settings.as_bytes());
        assert_eq!(status, Status::OK, "{:?}", loader.faults());
        assert_eq!(
            std::fs::read(own_dir().join("tone.wav")).unwrap(),
            tone::wav()
        );
        assert!(
            logged(&loader, "plays at volume 0.2"),
            "{:?} {:?}",
            loader.logs(),
            loader.features()
        );
        assert!(!logged(&loader, "no creature_sound"), "{:?}", loader.logs());
    }

    #[test]
    fn a_settings_file_that_does_not_read_is_named_in_the_log() {
        let (_, loader) = start_on(on(Side::CLIENT), &[0xff, 0xfe, 0x00]);
        assert!(logged(&loader, "the defaults apply"), "{:?}", loader.logs());
    }

    #[test]
    fn the_server_actions_and_the_input_read_say_why_they_are_off() {
        let (status, loader) = start(Side::CLIENT, false);
        assert_eq!(status, Status::OK);
        let off = |loader: &enshwick::test::Installed, part: &str| {
            loader
                .features()
                .iter()
                .any(|(name, reason)| name == part && reason.is_some())
        };
        assert!(off(&loader, "server actions"), "{:?}", loader.features());
        assert!(off(&loader, "input read"), "{:?}", loader.features());
        drop(loader);
        let (_, loader) = start_on(on(Side::CLIENT).world().player_input(), b"");
        assert!(!off(&loader, "server actions"), "{:?}", loader.features());
        assert!(!off(&loader, "input read"), "{:?}", loader.features());
    }

    #[test]
    fn the_read_line_names_the_held_controls() {
        let (_, loader) = start_on(on(Side::CLIENT).player_input(), b"");
        loader.hold_locally(Some(1 << 30));
        let _drawn = loader.draw_frame();
        assert!(logged(&loader, "held Some([Block])"), "{:?}", loader.logs());
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
    fn a_chest_of_the_template_the_settings_name_shows_its_host_position() {
        use enshwick::content::Guid;

        let chest = Guid::named("test:chest");
        let settings = format!("chest_template = \"{chest}\"\n");
        let (_, loader) = start_on(on(Side::CLIENT), settings.as_bytes());
        loader.host_screen(Some(enshwick::Entity(512)));
        // A Safe is made from the vanilla chest's template, not its item's.
        loader.read_screen_host(Guid::named("enshwick.personal-safes:safe"), [0.0; 3]);
        assert!(loader.draw_screen(HudScreenType::StorageChest).is_empty());
        loader.read_screen_host(chest, [1.25, 2.5, -3.0]);
        let drawn = loader.draw_screen(HudScreenType::StorageChest);
        let labels: Vec<String> = drawn
            .iter()
            .flat_map(|(_, widgets)| widgets)
            .filter_map(|widget| match widget {
                Drawn::Label { text, .. } => Some(text.clone()),
                _ => None,
            })
            .collect();
        assert_eq!(labels, ["Chest", "1.2, 2.5, -3.0"]);
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

    #[test]
    fn a_placement_within_5_m_of_an_altar_is_refused_with_the_reason() {
        use enshwick::abi::{AltarInfo, BuildCheck, Header};

        let (_, loader) = start(Side::SERVER, false);
        let mut altar = AltarInfo::EMPTY;
        altar.entity = enshwick::Entity(1);
        altar.position = [10.0, 0.0, 0.0];
        loader.set_altars(&[altar]);
        let at = |x: f64| BuildCheck {
            header: Header::of::<BuildCheck>(),
            player: enshwick::Entity(2),
            item: enshwick::ItemId::new(7),
            position: enshwick::ecs::fixed([x, 0.0, 0.0]),
            reason: core::ptr::null_mut(),
        };
        assert_eq!(
            loader.build(&at(14.9)),
            Err(Some("Too close to the altar".to_owned()))
        );
        assert_eq!(loader.build(&at(15.1)), Ok(()));
        assert_eq!(loader.build(&at(4.0)), Ok(()));
    }
}
