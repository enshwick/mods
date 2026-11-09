//! The client's panels beside the game's screens: a "Hello" button beside
//! the marked survivor's dialog, a line beside the Flame Altar screen, and a
//! "Chest" panel with the host's position beside the screen of a storage
//! chest of the template the settings name. Each
//! screen change is logged with its host.

use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};

use enshwick::Load;
use enshwick::actions::Actions;
use enshwick::ui::{Edge, HudScreenType, Label, Style};

use crate::server::HELLO;
use crate::{SURVIVOR, Settings};

/// The line beside the Flame Altar screen.
const ALTAR_LINE: &str = "Test UI: a line beside the Flame Altar screen";

/// Adds the panels and the screen log; each turns off alone.
pub(crate) fn register(load: &mut Load<'_>, actions: &Actions, settings: &Settings) {
    // The dialog hides the prompt, so the last prompt seen names the survivor.
    let marked = Arc::new(AtomicBool::new(false));
    let seen = Arc::clone(&marked);
    let _prompt = load.feature("survivor prompt", |load| {
        load.on_frame(move |frame| {
            if let Some(prompt) = frame.prompt() {
                seen.store(prompt.title == SURVIVOR, Ordering::Relaxed);
            }
        })
    });
    let actions = actions.clone();
    let _hello = load.feature("survivor button", |load| {
        load.extend_screen(HudScreenType::NpcDialog, Edge::Right, move |panel| {
            if marked.load(Ordering::Relaxed)
                && panel.button("Hello")
                && let Err(why) = actions.run(HELLO, &[])
            {
                enshwick::warn!("hello did not reach the server: {why}");
            }
        })
    });
    let _altar = load.feature("altar line", |load| {
        load.extend_screen(HudScreenType::FlameAltar, Edge::Right, |panel| {
            panel.label(Label::new(ALTAR_LINE).style(Style::Secondary).wrap());
        })
    });
    // The host's template is the placed chest's, which a mod's item that
    // places a vanilla chest shares with it, so the settings name it.
    if let Some(chest) = crate::guid(&settings.chest_template, "chest_template") {
        let _chest = load.feature("chest panel", |load| {
            load.extend_screen_for(
                HudScreenType::StorageChest,
                Edge::Right,
                &[chest],
                |panel| {
                    panel.label("Chest");
                    if let Some([x, y, z]) = panel.screen_host_position() {
                        panel.label(format!("{x:.1}, {y:.1}, {z:.1}").as_str());
                    }
                },
            )
        });
    }
    let _log = load.feature("screen log", |load| {
        load.on_screen_changed(|change| {
            enshwick::info!(
                "screen {:?} -> {:?}, host {:?}",
                change.before,
                change.now,
                change.host
            );
        })
    });
}
