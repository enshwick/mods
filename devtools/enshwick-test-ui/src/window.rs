//! The client's research window, on Shift+F10: buttons that play the
//! game's and the mod's sounds and run the server's actions, and two input
//! hints for the interact control. Once a second while in a game, it logs
//! the held controls, the open screen, its host and template, the prompt
//! and the hints, each time the line changes.

use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::{Duration, Instant};

use enshwick::actions::Actions;
use enshwick::content::id;
use enshwick::controls::{Control, Key};
use enshwick::input::Input;
use std::path::Path;

use enshwick::content::Id;
use enshwick::sound::{
    AddedSound, NewSound, NewTake, SoundChange, SoundContainerId, SoundContainerScene, SoundId,
    Take, Takes,
};
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
/// The file of the tone that replaces a vanilla recording.
const REPLACED_FILE: &str = "replaced.wav";
/// The folder in the mod's folder whose folders are sounds of their files.
const SOUNDS_DIR: &str = "sounds";
/// The extensions of the files a folder sound takes, in lower case.
const AUDIO: [&str; 4] = ["wav", "flac", "ogg", "mp3"];
/// The volume of the quiet vanilla container.
const QUIET: f32 = 0.2;
/// How often the reads are looked at.
const READ_EVERY: Duration = Duration::from_secs(1);

/// Writes a tone of `pitch` hertz into the mod's folder as `file`.
fn write_tone(load: &Load<'_>, file: &str, pitch: f64) -> Result<(), Off> {
    let path = load.mod_dir()?.join(file);
    std::fs::write(&path, crate::tone::wav(pitch))
        .map_err(|e| Off::new(format!("{}: {e}", path.display())))
}

/// Writes the tone into the mod's folder and adds it as a sound of the
/// interface scene.
fn tone(load: &mut Load<'_>) -> Result<AddedSound, Off> {
    write_tone(load, TONE_FILE, crate::tone::TONE)?;
    load.add_sound(NewSound {
        container: SoundChange {
            scene: Some(SoundContainerScene::Ui),
            ..SoundChange::default()
        },
        ..NewSound::new(id!("tone"), TONE_FILE)
    })
}

/// Makes the vanilla container `container` play only the tone's recording.
fn tone_takes(load: &mut Load<'_>, container: SoundContainerId, tone: SoundId) -> Result<(), Off> {
    let change = SoundChange {
        takes: Some(Takes::Replace(vec![Take::new(tone)])),
        ..SoundChange::default()
    };
    load.change_sound(container, change)
}

/// Writes the higher tone and plays it wherever the game plays `sound`.
fn replace(load: &mut Load<'_>, sound: SoundId) -> Result<(), Off> {
    write_tone(load, REPLACED_FILE, crate::tone::REPLACED)?;
    load.replace_sound(sound, REPLACED_FILE)
}

/// The names of the folders in `dir`, or of its files with `files`, sorted;
/// a `dir` that is not there has none, and one that does not list is logged.
fn names(dir: &Path, files: bool) -> Vec<String> {
    let entries = match std::fs::read_dir(dir) {
        Ok(entries) => entries,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Vec::new(),
        Err(e) => {
            enshwick::warn!("{}: {e}", dir.display());
            return Vec::new();
        }
    };
    let mut names: Vec<String> = entries
        .filter_map(|entry| {
            entry
                .inspect_err(|e| enshwick::warn!("{}: {e}", dir.display()))
                .ok()
        })
        .filter(|entry| entry.path().is_file() == files)
        .map(|entry| entry.file_name().to_string_lossy().into_owned())
        .collect();
    names.sort();
    names
}

/// Whether `file` is one of the audio files a folder sound takes.
fn is_audio(file: &str) -> bool {
    Path::new(file)
        .extension()
        .and_then(|extension| extension.to_str())
        .is_some_and(|extension| AUDIO.iter().any(|a| a.eq_ignore_ascii_case(extension)))
}

/// The content id of the sound of `folder`, `<mod>:sounds/<folder>`, apart
/// from the mod's own sounds; a dot would name another folder's take.
fn folder_id(namespace: &'static str, folder: &str) -> Result<Id, Off> {
    // Ids live for the process, as a data pack's do.
    let name: &'static str = Box::leak(format!("{SOUNDS_DIR}/{folder}").into_boxed_str());
    Id::try_of(namespace, name)
        .filter(|_| !folder.contains('.'))
        .ok_or_else(|| Off::new("a folder sound's name is a-z, 0-9, _ or -"))
}

/// A folder sound: its folder, its button and its container.
struct FolderSound {
    folder: String,
    button: String,
    container: SoundContainerId,
}

/// Adds each folder of `sounds/` in the mod's folder as an interface sound
/// whose takes are its audio files, in name order; each turns off alone.
fn folder_sounds(load: &mut Load<'_>) -> Vec<FolderSound> {
    let Ok(dir) = load.mod_dir().map(|dir| dir.join(SOUNDS_DIR)) else {
        return Vec::new();
    };
    let mut sounds = Vec::new();
    for folder in names(&dir, false) {
        let takes: Vec<NewTake> = names(&dir.join(&folder), true)
            .into_iter()
            .filter(|file| is_audio(file))
            .map(|file| NewTake::new(format!("{SOUNDS_DIR}/{folder}/{file}")))
            .collect();
        let added = load.feature(&format!("sound {folder}"), |load| {
            let id = folder_id(id!("tone").namespace(), &folder)?;
            load.add_sound(NewSound {
                takes,
                container: SoundChange {
                    scene: Some(SoundContainerScene::Ui),
                    ..SoundChange::default()
                },
                ..NewSound::new(id, "")
            })
        });
        if let Some(added) = added {
            enshwick::info!(
                "{SOUNDS_DIR}/{folder} is sound container {}",
                added.container.guid()
            );
            sounds.push(FolderSound {
                button: format!("Play {folder}"),
                folder,
                container: added.container,
            });
        }
    }
    sounds
}

/// The container a setting names, or `None` with the reason in the log.
fn container(setting: &str, name: &str) -> Option<SoundContainerId> {
    crate::guid(setting, name).map(SoundContainerId::from_guid)
}

/// Whether the window shows: only in a game, where its key fires.
const fn shows(open: bool, screen: Screen) -> bool {
    open && matches!(screen, Screen::InGame)
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
    input: Option<Input>,
    tone: Option<SoundContainerId>,
    creature: Option<SoundContainerId>,
    tone_take: Option<SoundContainerId>,
    replaced: Option<SoundContainerId>,
    folders: Vec<FolderSound>,
    hints: String,
    read_at: Option<Instant>,
    last: String,
}

impl Window {
    /// Logs the reads once a second, when they changed.
    fn read(&mut self, frame: &Frame<'_>) {
        let waits = self.read_at.is_some_and(|at| at.elapsed() < READ_EVERY);
        if frame.screen() != Screen::InGame || waits {
            return;
        }
        self.read_at = Some(Instant::now());
        let held = self
            .input
            .and_then(Input::local_held)
            .map(|held| held.iter().collect::<Vec<_>>());
        let line = format!(
            "held {:?}, screen {:?}, host {:?} template {:?}, prompt {:?}, hints {}",
            held,
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
        let open = self.open.load(Ordering::Relaxed);
        if !shows(open, frame.screen()) {
            // Left open, it would cover the menus, where no key closes it.
            if open {
                self.open.store(false, Ordering::Relaxed);
            }
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
            if w.button("Play tone take") {
                match self.tone_take {
                    Some(container) => played(
                        &format!("sound container {} with the tone's take", container.guid()),
                        w.play(container),
                    ),
                    None => enshwick::warn!("the tone take is off; see the log at start"),
                }
            }
            if w.button("Play replaced recording") {
                match self.replaced {
                    Some(container) => played(
                        &format!(
                            "sound container {} of the replaced recording",
                            container.guid()
                        ),
                        w.play(container),
                    ),
                    None => enshwick::warn!(
                        "no replaced recording to play: replaced_sound or replaced_container is empty, or the replacement is off; see the log at start"
                    ),
                }
            }
            for sound in &self.folders {
                if w.button(sound.button.as_str()) {
                    played(
                        &format!("{SOUNDS_DIR}/{}", sound.folder),
                        w.play(sound.container),
                    );
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
    let added = load.feature("mod sound", tone).inspect(|added| {
        enshwick::info!(
            "the tone is sound container {}, recording {:?}",
            added.container.guid(),
            added.takes.first().map(|take| take.guid())
        );
    });
    let tone = added.as_ref().map(|added| added.container);
    let recording = added.and_then(|added| added.takes.first().copied());
    let tone_take = container(&settings.tone_takes, "tone_takes").and_then(|container| {
        load.feature("tone take", |load| {
            let recording = recording.ok_or_else(|| Off::new("the mod sound is off"))?;
            tone_takes(load, container, recording)
        })
        .map(|()| container)
    });
    let replaced = crate::guid(&settings.replaced_sound, "replaced_sound")
        .and_then(|sound| {
            load.feature("replaced recording", |load| {
                replace(load, SoundId::from_guid(sound))
            })
        })
        .and_then(|()| container(&settings.replaced_container, "replaced_container"));
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
        input: load.feature("input read", Load::input),
        tone,
        creature: container(&settings.creature_sound, "creature_sound"),
        tone_take,
        replaced,
        folders: folder_sounds(load),
        hints: "not drawn yet".to_owned(),
        read_at: None,
        last: String::new(),
    };
    let _window = load.feature("window", |load| {
        load.on_frame(move |frame| window.draw(frame))
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_window_shows_only_in_a_game() {
        assert!(shows(true, Screen::InGame));
        assert!(!shows(false, Screen::InGame));
        assert!(!shows(true, Screen::Menu), "its key cannot close it there");
        assert!(!shows(true, Screen::Loading));
    }

    #[test]
    fn a_folder_sound_takes_only_audio_files_in_any_case() {
        for file in ["1.wav", "2.FLAC", "a.b.Ogg", "x.mp3"] {
            assert!(is_audio(file), "{file}");
        }
        for file in ["notes.txt", "desktop.ini", "wav", "a.wav.bak"] {
            assert!(!is_audio(file), "{file}");
        }
    }

    #[test]
    fn a_folder_sound_s_id_stays_apart_from_the_mod_s_own_sounds_and_takes() {
        let tone = id!("tone");
        let folder = folder_id(tone.namespace(), "tone").unwrap();
        assert_eq!(
            folder.to_string(),
            format!("{}:sounds/tone", tone.namespace())
        );
        assert_ne!(folder.guid(), tone.guid());
        assert!(folder_id(tone.namespace(), "horn.sound").is_err());
        assert!(folder_id(tone.namespace(), "Horn Blast").is_err());
    }
}
