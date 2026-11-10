//! Backups of the world while it runs: the mod holds the world's saves,
//! copies the save with the world's mod settings and data to a folder named
//! by the time, off the game thread, lets the saves go again, also after an
//! error, and keeps the newest backups the world's settings name.

use core::time::Duration;
use std::fs;
use std::io;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::sync::atomic::Ordering;

use enshwick::actions::{Action, Outcome, Request};
use enshwick::i18n::Language;
use enshwick::{Off, PlayerKey};

use crate::{Tools, ids, journal, texts};

/// The backups' folder in the world's data folder unless the settings name
/// another.
const FOLDER: &str = "backups";

/// The suffix of a backup's folder while it is copied.
const PARTIAL: &str = ".partial";

/// The suffix of the world's mod data folder beside its save.
const DATA_SUFFIX: &str = "-mod-data";

/// How long the copy waits for the hold to take effect: the game sees it
/// after its next step.
const SETTLE: Duration = Duration::from_secs(1);

/// How often a copy starts again when the save changed under it.
const TRIES: u32 = 3;

/// The extension of the world's save file.
const SAVE_EXTENSION: &str = "ksd";

/// The suffix of the world's mod settings folder beside its save.
const SETTINGS_SUFFIX: &str = "-mod-settings";

/// How long a copy waits for a save already being written to end; past
/// it, the backup fails rather than copy a save half written.
const SAVE_WAIT: Duration = Duration::from_secs(120);

/// How often a copy looks whether the save being written has ended.
const SAVE_LOOK: Duration = Duration::from_millis(100);

/// Declares the backup action.
pub(crate) fn add(tools: &Arc<Tools>) -> Result<(), Off> {
    tools.add(
        Action::new(ids::BACKUP, texts::ACTION_BACKUP).help(texts::ACTION_BACKUP_HELP),
        backup,
    )
}

/// What a backup copies and where to.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Plan {
    /// The folder that holds the world's save.
    pub saves: PathBuf,
    /// The world's key, the save's name without its extension.
    pub stem: String,
    /// The folder each backup goes into, one folder per backup.
    pub root: PathBuf,
    /// The default backups' folder in the world's data folder, never
    /// copied, also when backups go elsewhere now.
    pub default: PathBuf,
    /// How many backups to keep.
    pub kept: usize,
}

/// Starts a backup: the saves are held now, and the copy runs once the hold
/// took effect; the reply says it started, and the caller reads how it
/// ended.
fn backup(tools: &Tools, request: &Request<'_>) -> Outcome {
    let (Some(plan), Some(me)) = (tools.backup_plan(), tools.me.upgrade()) else {
        return Outcome::refused(request.say(texts::REPLY_BACKUP_NONE, &[]));
    };
    if tools.backing_up.swap(true, Ordering::AcqRel) {
        return Outcome::refused(request.say(texts::REPLY_BACKUP_BUSY, &[]));
    }
    if let Err(why) = tools.world.hold_saves(true) {
        tools.backing_up.store(false, Ordering::Release);
        return Tools::failed(request, ids::BACKUP, &why);
    }
    let caller = Caller {
        key: request.player.key,
        language: request.language,
    };
    tools
        .scheduler
        .after(SETTLE, move || Arc::clone(&me).copy_aside(&plan, caller));
    // The console and a remote console get no notice: the logs keep the end.
    let started = if caller.key.0 == 0 {
        texts::REPLY_BACKUP_STARTED_LOG
    } else {
        texts::REPLY_BACKUP_STARTED
    };
    Outcome::done(request.say(started, &[]))
}

/// Who asked for a backup and the language they read its end in; account 0
/// for the console and a remote console.
#[derive(Debug, Clone, Copy)]
struct Caller {
    key: PlayerKey,
    language: Language,
}

impl Tools {
    /// What a backup of the running world copies, from the world's data
    /// folder, `<saves>/<stem>-mod-data/<mod>`; `None` while no world runs
    /// or when the folder lies elsewhere.
    fn backup_plan(&self) -> Option<Plan> {
        let stem = self.world.key()?;
        let data = self.world.data_dir()?;
        let mod_data = data.parent()?;
        if mod_data.file_name()?.to_str()? != format!("{stem}{DATA_SUFFIX}") {
            return None;
        }
        let settings = self.settings.load();
        let default = data.join(FOLDER);
        let root = if settings.backup_dir.is_empty() {
            default.clone()
        } else if !crate::settings::beside_the_save(&settings.backup_dir) {
            return None;
        } else {
            // A folder of the world's own, as every world in the saves'
            // folder reads the same setting and pruning counts by name.
            mod_data.parent()?.join(&settings.backup_dir).join(&stem)
        };
        Some(Plan {
            saves: mod_data.parent()?.to_path_buf(),
            stem,
            root,
            default,
            kept: usize::try_from(settings.backups_kept).unwrap_or(usize::MAX),
        })
    }

    /// Copies the world aside on a thread of its own, once a save being
    /// written has ended, lets the saves go and tells the caller how it went.
    fn copy_aside(self: Arc<Self>, plan: &Plan, caller: Caller) {
        let (tools, copying) = (Arc::clone(&self), plan.clone());
        let spawned = std::thread::Builder::new()
            .name("admin-tools backup".into())
            .spawn(move || {
                // A panic in the copy still lets the saves go.
                let copied = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                    tools.copy_held(&copying)
                }))
                .unwrap_or_else(|_| Err(io::Error::other("the copy stopped on an error")));
                tools.finish_backup(&copying, copied, caller);
            });
        if let Err(why) = spawned {
            self.finish_backup(plan, Err(why), caller);
        }
    }

    /// Waits until no save is being written while the world runs, at most
    /// [`SAVE_WAIT`]: the hold keeps a new save from starting, never one
    /// under way. A world that stopped meanwhile is not copied, as its final
    /// save may be under way; past the wait the save's end is taken as
    /// missed, so the next backup does not wait for it again.
    fn wait_for_save(&self) -> io::Result<()> {
        let waited = wait_while(
            || self.saving.load(Ordering::Acquire) && self.running.load(Ordering::Relaxed),
            SAVE_WAIT,
        );
        if waited.is_err() {
            self.saving.store(false, Ordering::Release);
        }
        waited?;
        if !self.running.load(Ordering::Relaxed) {
            return Err(io::Error::other("the world stopped before the copy"));
        }
        Ok(())
    }

    /// Copies the world while its saves stay held, after a save under way
    /// ended and again when the save on disk changed during the copy; the
    /// backup's folder.
    fn copy_held(&self, plan: &Plan) -> io::Result<PathBuf> {
        self.wait_for_save()?;
        for _ in 0..TRIES {
            let before = self.world.save_info();
            let into = copy(plan, &folder_name(journal::now()))?;
            if self.world.save_info() == before {
                return Ok(into);
            }
            fs::remove_dir_all(&into)?;
        }
        Err(io::Error::other("the save kept changing during the copy"))
    }

    /// Lets the saves go, prunes old backups and reports the result: to the
    /// server's log and the audit log, and to a player caller in their
    /// language.
    fn finish_backup(&self, plan: &Plan, copied: io::Result<PathBuf>, caller: Caller) {
        if let Err(why) = self.world.hold_saves(false) {
            enshwick::warn!("backup: saves not let go: {why}");
        }
        let (key, value, said) = match copied {
            Ok(into) => {
                if let Err(why) = prune(&plan.root, plan.kept) {
                    enshwick::warn!("backup: old backups not removed: {why}");
                }
                let path = into.display().to_string();
                enshwick::info!("backup written to {path}");
                (texts::NOTICE_BACKUP_DONE, texts::value::PATH, path)
            }
            Err(why) => {
                let why = why.to_string();
                enshwick::warn!("backup failed: {why}");
                (texts::NOTICE_BACKUP_FAILED, texts::value::WHY, why)
            }
        };
        self.log(&["server", "", ids::BACKUP, "", &said]);
        self.backing_up.store(false, Ordering::Release);
        if caller.key.0 == 0 {
            return;
        }
        let line = self.texts.fill(key, caller.language, &[(value, &said)]);
        let world = self.world;
        self.scheduler.next_tick(move || {
            if let Some(player) = world.player_by_key(caller.key) {
                let _shown = world.notify(&player, &line);
            }
        });
    }
}

/// Waits until `saving` is false, at most `limit`, looking every
/// [`SAVE_LOOK`]; past it, the error that a save is still being written.
fn wait_while(saving: impl Fn() -> bool, limit: Duration) -> io::Result<()> {
    let deadline = std::time::Instant::now() + limit;
    while saving() {
        if std::time::Instant::now() >= deadline {
            return Err(io::Error::other("the game is still writing the save"));
        }
        std::thread::sleep(SAVE_LOOK);
    }
    Ok(())
}

/// A backup folder's name for `seconds` since 1970: the UTC time, sortable,
/// with no character a file system refuses.
pub(crate) fn folder_name(seconds: u64) -> String {
    journal::utc(seconds).replace(' ', "_").replace(':', "-")
}

/// Whether `name` is a backup folder's name, so pruning removes nothing
/// else in a folder the admin named.
fn is_backup(name: &str) -> bool {
    name.len() == "YYYY-MM-DD_HH-MM-SS".len()
        && name.bytes().enumerate().all(|(at, byte)| match at {
            4 | 7 | 13 | 16 => byte == b'-',
            10 => byte == b'_',
            _ => byte.is_ascii_digit(),
        })
}

/// The entries beside the save of the world `stem` that restore it: the
/// save and its mods' settings and data. The game's rolling copies in
/// `backup/` and the folders the loader renamed aside, `.old-<stamp>`, stay
/// out.
fn own_entries(stem: &str) -> [String; 3] {
    [
        format!("{stem}.{SAVE_EXTENSION}"),
        format!("{stem}{SETTINGS_SUFFIX}"),
        format!("{stem}{DATA_SUFFIX}"),
    ]
}

/// Copies the world's files of `plan` into a new folder `name` under its
/// root, leaving the root out where it lies inside them; the new folder.
/// The copy is made under a [`PARTIAL`] name and renamed when whole, so a
/// copy that fails or a server that stops part-way leaves no folder that
/// counts as a backup.
pub(crate) fn copy(plan: &Plan, name: &str) -> io::Result<PathBuf> {
    fs::create_dir_all(&plan.root)?;
    let into = plan.root.join(name);
    if into.exists() {
        return Err(io::Error::new(
            io::ErrorKind::AlreadyExists,
            format!("{} is there already", into.display()),
        ));
    }
    let partial = plan.root.join(format!("{name}{PARTIAL}"));
    if partial.exists() {
        fs::remove_dir_all(&partial)?;
    }
    fs::create_dir(&partial)?;
    copy_into(plan, &partial)
        .and_then(|()| fs::rename(&partial, &into))
        .inspect_err(|_| {
            let _removed = fs::remove_dir_all(&partial);
        })?;
    Ok(into)
}

/// Copies the world's files of `plan` into the folder `into`.
fn copy_into(plan: &Plan, into: &Path) -> io::Result<()> {
    let skip: Vec<PathBuf> = [&plan.root, &plan.default]
        .into_iter()
        .filter_map(|dir| fs::canonicalize(dir).ok())
        .collect();
    // A backup without the save restores nothing, so a missing one fails it.
    fs::metadata(plan.saves.join(format!("{}.{SAVE_EXTENSION}", plan.stem)))?;
    for name in own_entries(&plan.stem) {
        let from = plan.saves.join(&name);
        if fs::symlink_metadata(&from).is_ok() {
            copy_tree(&from, &into.join(&name), &skip)?;
        }
    }
    Ok(())
}

/// Copies `from` to `to`, a folder with all it holds, leaving out the
/// folders `skip`.
fn copy_tree(from: &Path, to: &Path, skip: &[PathBuf]) -> io::Result<()> {
    let kind = fs::symlink_metadata(from)?.file_type();
    if kind.is_symlink() {
        return Ok(());
    }
    if !kind.is_dir() {
        return fs::copy(from, to).map(drop);
    }
    if skip.contains(&fs::canonicalize(from)?) {
        return Ok(());
    }
    fs::create_dir_all(to)?;
    for entry in fs::read_dir(from)? {
        let entry = entry?;
        copy_tree(&entry.path(), &to.join(entry.file_name()), skip)?;
    }
    Ok(())
}

/// Removes the oldest backups in `root` past the newest `kept`.
pub(crate) fn prune(root: &Path, kept: usize) -> io::Result<()> {
    let mut names: Vec<String> = fs::read_dir(root)?
        .filter_map(Result::ok)
        .filter(|entry| entry.file_type().is_ok_and(|kind| kind.is_dir()))
        .filter_map(|entry| entry.file_name().into_string().ok())
        .filter(|name| is_backup(name))
        .collect();
    names.sort_unstable();
    let old = names.len().saturating_sub(kept);
    for name in &names[..old] {
        fs::remove_dir_all(root.join(name))?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use std::sync::atomic::AtomicBool;

    use super::*;

    fn scratch(name: &str) -> PathBuf {
        let dir =
            std::env::temp_dir().join(format!("admin-tools-backup-{name}-{}", std::process::id()));
        let _gone = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();
        dir
    }

    #[test]
    fn a_backup_takes_the_save_and_its_mod_folders_and_no_rolling_copy_or_old_folder() {
        let saves = scratch("own");
        for folder in [
            "world0-mod-settings",
            "world0-mod-data",
            "world0-mod-data.old-2026-11-02-10-00-00",
            "world0-mod-settings.old-2026-11-02-10-00-00",
            "backup",
        ] {
            fs::create_dir_all(saves.join(folder)).unwrap();
        }
        fs::write(saves.join("world0.ksd"), "save").unwrap();
        fs::write(saves.join("world01.ksd"), "another").unwrap();
        fs::write(saves.join("backup/world0.ksd.backup-0"), "rolling").unwrap();
        let plan = Plan {
            saves: saves.clone(),
            stem: "world0".into(),
            root: saves.join("out"),
            default: PathBuf::new(),
            kept: 1,
        };
        let into = copy(&plan, "2026-01-01_00-00-00").unwrap();
        let mut taken: Vec<String> = fs::read_dir(&into)
            .unwrap()
            .map(|entry| entry.unwrap().file_name().into_string().unwrap())
            .collect();
        taken.sort();
        assert_eq!(
            taken,
            ["world0-mod-data", "world0-mod-settings", "world0.ksd"]
        );
        fs::remove_dir_all(&saves).unwrap();
    }

    #[test]
    fn backup_names_sort_by_time_and_nothing_else_counts_as_one() {
        assert_eq!(folder_name(1_793_793_600), "2026-11-04_12-00-00");
        assert!(is_backup(&folder_name(0)));
        assert!(folder_name(1) < folder_name(60));
        assert!(!is_backup("photos"));
        assert!(!is_backup("2026-11-04 12-00-00"));
        assert!(!is_backup("2026-11-04_12-00-0x"));
    }

    #[test]
    fn a_copy_waits_for_a_save_to_end_and_fails_when_it_never_does() {
        let saving = AtomicBool::new(false);
        assert!(
            wait_while(|| saving.load(Ordering::Acquire), Duration::ZERO).is_ok(),
            "no save, no wait"
        );
        saving.store(true, Ordering::Release);
        let started = std::time::Instant::now();
        assert!(
            wait_while(
                || saving.load(Ordering::Acquire),
                Duration::from_millis(250)
            )
            .is_err()
        );
        assert!(started.elapsed() >= Duration::from_millis(250));
        let written = Arc::new(AtomicBool::new(true));
        let writer = Arc::clone(&written);
        let save = std::thread::spawn(move || {
            std::thread::sleep(Duration::from_millis(150));
            writer.store(false, Ordering::Release);
        });
        assert!(wait_while(|| written.load(Ordering::Acquire), Duration::from_secs(5)).is_ok());
        save.join().unwrap();
    }

    #[test]
    fn a_copy_takes_the_world_s_files_and_leaves_out_its_own_folder() {
        let saves = scratch("copy");
        let data = saves.join("world0-mod-data").join("admin-tools");
        fs::create_dir_all(&data).unwrap();
        fs::create_dir_all(saves.join("world0-mod-settings").join("admin-tools")).unwrap();
        fs::write(saves.join("world0.ksd"), "save").unwrap();
        fs::write(saves.join("world1.ksd"), "other").unwrap();
        fs::write(data.join("accounts.toml"), "bans = []").unwrap();
        fs::write(
            saves
                .join("world0-mod-settings")
                .join("admin-tools")
                .join("settings.toml"),
            "motd = \"hi\"",
        )
        .unwrap();
        let plan = Plan {
            saves: saves.clone(),
            stem: "world0".into(),
            root: data.join(FOLDER),
            default: data.join(FOLDER),
            kept: 2,
        };
        let first = copy(&plan, "2026-01-01_00-00-00").unwrap();
        assert_eq!(
            fs::read_to_string(first.join("world0.ksd")).unwrap(),
            "save"
        );
        assert!(!first.join("world1.ksd").exists());
        assert!(
            first
                .join("world0-mod-settings/admin-tools/settings.toml")
                .exists()
        );
        assert!(
            first
                .join("world0-mod-data/admin-tools/accounts.toml")
                .exists()
        );
        assert!(
            !first
                .join("world0-mod-data/admin-tools")
                .join(FOLDER)
                .exists(),
            "a backup never holds the backups"
        );
        let _second = copy(&plan, "2026-01-02_00-00-00").unwrap();
        let _third = copy(&plan, "2026-01-03_00-00-00").unwrap();
        fs::create_dir_all(plan.root.join("keep-me")).unwrap();
        prune(&plan.root, plan.kept).unwrap();
        let mut left: Vec<String> = fs::read_dir(&plan.root)
            .unwrap()
            .map(|entry| entry.unwrap().file_name().into_string().unwrap())
            .collect();
        left.sort();
        assert_eq!(
            left,
            ["2026-01-02_00-00-00", "2026-01-03_00-00-00", "keep-me"]
        );
        fs::remove_dir_all(&saves).unwrap();
    }

    #[test]
    fn backups_in_the_default_folder_stay_out_once_backups_go_elsewhere() {
        let saves = scratch("moved");
        let data = saves.join("w-mod-data").join("admin-tools");
        let old = data.join(FOLDER).join("2026-01-01_00-00-00");
        fs::create_dir_all(&old).unwrap();
        fs::write(old.join("w.ksd"), "old").unwrap();
        fs::write(saves.join("w.ksd"), "save").unwrap();
        let plan = Plan {
            saves: saves.clone(),
            stem: "w".into(),
            root: saves.join("w-backups").join("w"),
            default: data.join(FOLDER),
            kept: 1,
        };
        let into = copy(&plan, "2026-01-02_00-00-00").unwrap();
        assert!(into.join("w.ksd").exists());
        assert!(!into.join("w-mod-data/admin-tools").join(FOLDER).exists());
        assert!(!into.join("w-backups/w").exists());
        fs::remove_dir_all(&saves).unwrap();
    }

    #[test]
    fn a_copy_that_fails_part_way_leaves_no_folder_that_counts_as_a_backup() {
        let dir = scratch("half");
        let plan = Plan {
            saves: dir.join("missing"),
            stem: "w".into(),
            root: dir.join("out"),
            default: PathBuf::new(),
            kept: 1,
        };
        assert!(copy(&plan, "2026-01-01_00-00-00").is_err());
        assert_eq!(fs::read_dir(&plan.root).unwrap().count(), 0);
        fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn a_copy_gets_its_backup_name_only_once_whole_and_a_crashed_one_never_counts() {
        let saves = scratch("atomic");
        fs::write(saves.join("w.ksd"), "save").unwrap();
        let plan = Plan {
            saves: saves.clone(),
            stem: "w".into(),
            root: saves.join("out"),
            default: PathBuf::new(),
            kept: 1,
        };
        let name = "2026-01-01_00-00-00";
        let crashed = plan.root.join(format!("{name}{PARTIAL}"));
        fs::create_dir_all(crashed.join("half")).unwrap();
        assert!(!is_backup(&format!("{name}{PARTIAL}")));
        let into = copy(&plan, name).unwrap();
        assert_eq!(into, plan.root.join(name));
        assert_eq!(fs::read_to_string(into.join("w.ksd")).unwrap(), "save");
        assert!(
            !into.join("half").exists(),
            "the crashed copy is not mixed in"
        );
        assert!(!crashed.exists());
        fs::remove_dir_all(&saves).unwrap();
    }

    #[test]
    fn a_second_backup_in_the_same_second_is_refused_rather_than_mixed() {
        let saves = scratch("twice");
        fs::write(saves.join("w.ksd"), "save").unwrap();
        let plan = Plan {
            saves: saves.clone(),
            stem: "w".into(),
            root: saves.join("out"),
            default: PathBuf::new(),
            kept: 1,
        };
        copy(&plan, "2026-01-01_00-00-00").unwrap();
        assert!(copy(&plan, "2026-01-01_00-00-00").is_err());
        fs::remove_dir_all(&saves).unwrap();
    }

    #[test]
    fn a_world_without_its_save_gets_no_backup() {
        let saves = scratch("no-save");
        fs::create_dir_all(saves.join("w-mod-settings")).unwrap();
        let plan = Plan {
            saves: saves.clone(),
            stem: "w".into(),
            root: saves.join("out"),
            default: PathBuf::new(),
            kept: 1,
        };
        assert!(copy(&plan, "2026-01-01_00-00-00").is_err());
        assert_eq!(fs::read_dir(&plan.root).unwrap().count(), 0);
        fs::remove_dir_all(&saves).unwrap();
    }
}
