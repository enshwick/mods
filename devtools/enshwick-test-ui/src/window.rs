//! The client's research window, on Shift+F10: buttons that play the
//! game's and the mod's sounds and run the server's actions, and two input
//! hints for the interact control. Once a second while in a game, it logs
//! the held controls, the open screen, its host and template, the prompt
//! and the hints, each time the line changes.

use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::{Duration, Instant};

use enshwick::actions::Actions;
use enshwick::content::{Guid, id};
use enshwick::controls::{Control, Key};
use enshwick::sound::{NewSound, SoundChange, SoundContainerId, SoundContainerScene};
use enshwick::typed::input_mapping::runtime::GameplayAction;
use enshwick::ui::{Frame, Screen, Sound};
use enshwick::{Load, Off};

use crate::Settings;
use crate::server::{MARK, SPAWN};

/// The window's key.
const KEY: &str = "Shift+F10";
/// The window's id among the mod's own and its title.
const WINDOW: &str = "test-ui";
const TITLE: &str = "Test UI";
/// The tone's file in the mod's folder.
const TONE_FILE: &str = "tone.wav";
/// The volume of the quiet vanilla container.
const QUIET: f32 = 0.2;
/// How often the reads are looked at.
const READ_EVERY: Duration = Duration::from_secs(1);

/// Writes the tone into the mod's folder and adds it as a sound of the
/// interface scene.
fn tone(load: &mut Load<'_>) -> Result<SoundContainerId, Off> {
    let path = load.mod_dir()?.join(TONE_FILE);
    std::fs::write(&path, crate::tone::wav())
        .map_err(|e| Off::new(format!("{}: {e}", path.display())))?;
    load.add_sound(NewSound {
        scene: SoundContainerScene::Ui,
        ..NewSound::new(id!("tone"), TONE_FILE)
    })
}

/// The container a setting names, or `None` with the reason in the log.
fn container(setting: &str, name: &str) -> Option<SoundContainerId> {
    if setting.is_empty() {
        enshwick::info!("no {name} in settings.toml");
        return None;
    }
    let guid = Guid::try_parse(setting);
    if guid.is_none() {
        enshwick::warn!("{name} {setting} is no GUID");
    }
    guid.map(SoundContainerId::from_guid)
}

/// Logs `result` of playing `what`.
fn played(what: &str, result: Result<(), Off>) {
    match result {
        Ok(()) => enshwick::info!("played {what}"),
        Err(off) => enshwick::warn!("{what} not played: {off}"),
    }
}

/// The two hints, labelled and bare, as the log line names them.
fn hints(frame: &mut Frame<'_>) -> String {
    let action = GameplayAction::ContextualAction;
    let mut hint = |label| match frame.input_hint(&action, label) {
        Ok(drawn) => format!("{drawn}"),
        Err(off) => format!("not drawn: {off}"),
    };
    let labelled = hint("Interact");
    let bare = hint("");
    format!("labelled {labelled}, bare {bare}")
}

/// What the window draws, kept between frames.
struct Window {
    open: Arc<AtomicBool>,
    actions: Actions,
    input: Option<enshwick::input::Input>,
    tone: Option<SoundContainerId>,
    creature: Option<SoundContainerId>,
    hints: String,
    read_at: Instant,
    last: String,
}

impl Window {
    /// Logs the reads once a second, when they changed.
    fn read(&mut self, frame: &Frame<'_>) {
        if frame.screen() != Screen::InGame || self.read_at.elapsed() < READ_EVERY {
            return;
        }
        self.read_at = Instant::now();
        let line = format!(
            "held {:?}, screen {:?}, host {:?} template {:?}, prompt {:?}, hints {}",
            self.input.and_then(enshwick::input::Input::local_held),
            frame.open_screen(),
            frame.screen_host(),
            frame.screen_host_template(),
            frame.prompt(),
            self.hints,
        );
        if line != self.last {
            enshwick::info!("{line}");
            self.last = line;
        }
    }

    fn run(&self, action: &str) {
        if let Err(why) = self.actions.run(action, &[]) {
            enshwick::warn!("{action} did not reach the server: {why}");
        }
    }

    fn draw(&mut self, frame: &mut Frame<'_>) {
        self.read(frame);
        if !self.open.load(Ordering::Relaxed) {
            return;
        }
        let mut close = false;
        frame.window(WINDOW, TITLE, |w| {
            if w.button("Play vanilla sound") {
                w.sound(Sound::Confirm);
                enshwick::info!("played the interface sound Confirm");
            }
            if w.button("Play mod sound") {
                match self.tone {
                    Some(tone) => played("the 440 Hz tone", w.play(tone)),
                    None => enshwick::warn!("the tone is off; see the log at start"),
                }
            }
            if w.button("Play creature sound") {
                match self.creature {
                    Some(creature) => played(
                        &format!("creature sound container {}", creature.guid()),
                        w.play(creature),
                    ),
                    None => enshwick::warn!("no creature_sound in settings.toml"),
                }
            }
            if w.button("Spawn kept and unsaved") {
                self.run(SPAWN);
            }
            if w.button("Mark the nearest survivor") {
                self.run(MARK);
            }
            self.hints = hints(w);
            close = w.button("Close");
        });
        if close {
            self.open.store(false, Ordering::Relaxed);
        }
    }
}

/// Adds the window, its key and its sounds; each part turns off alone.
pub(crate) fn register(load: &mut Load<'_>, actions: &Actions, settings: &Settings) {
    let tone = load
        .feature("mod sound", tone)
        .inspect(|tone| enshwick::info!("the tone is sound container {}", tone.guid()));
    if let Some(quiet) = container(&settings.quiet_sound, "quiet_sound") {
        let change = SoundChange {
            volume: Some(QUIET),
            ..SoundChange::default()
        };
        if load
            .feature("quiet sound", |load| load.change_sound(quiet, change))
            .is_some()
        {
            enshwick::info!("sound container {} plays at volume {QUIET}", quiet.guid());
        }
    }
    let open = Arc::new(AtomicBool::new(false));
    let toggle = Arc::clone(&open);
    let _key = load.feature("window key", |load| {
        load.control(
            Control::new(WINDOW, TITLE).key(Key::parse(KEY)?),
            move |_| {
                toggle.fetch_xor(true, Ordering::Relaxed);
            },
        )
    });
    let mut window = Window {
        open,
        actions: actions.clone(),
        input: load.input().ok(),
        tone,
        creature: container(&settings.creature_sound, "creature_sound"),
        hints: "not drawn yet".to_owned(),
        read_at: Instant::now(),
        last: String::new(),
    };
    let _window = load.feature("window", |load| {
        load.on_frame(move |frame| window.draw(frame))
    });
}
