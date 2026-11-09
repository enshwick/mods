//! The client's panels beside the game's screens: a "Hello" button beside
//! the marked survivor's dialog, a line beside the Flame Altar screen, and a
//! "Safe" panel with the host's position beside a Safe's chest screen. Each
//! screen change is logged with its host.

use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};

use enshwick::Load;
use enshwick::actions::Actions;
use enshwick::content::Id;
use enshwick::ui::{Edge, HudScreenType, Label, Style};

use crate::SURVIVOR;
use crate::server::HELLO;

/// The line beside the Flame Altar screen.
const ALTAR_LINE: &str = "Test UI: a line beside the Flame Altar screen";
/// The Personal Safes Safe, whose chest screen gets the "Safe" panel.
pub(crate) const SAFE: Id = Id::new("enshwick.personal-safes:safe");

/// Adds the panels and the screen log; each turns off alone.
pub(crate) fn register(load: &mut Load<'_>, actions: &Actions) {
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
    let _safe = load.feature("safe panel", |load| {
        load.extend_screen_for(
            HudScreenType::StorageChest,
            Edge::Right,
            &[SAFE.guid()],
            |panel| {
                panel.label("Safe");
                if let Some([x, y, z]) = panel.screen_host_position() {
                    panel.label(format!("{x:.1}, {y:.1}, {z:.1}").as_str());
                }
            },
        )
    });
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
