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
use enshwick::{Off, PlayerKey};

use crate::{Tools, ids, journal, texts};

/// The backups' folder in the world's data folder unless the settings name
/// another.
const FOLDER: &str = "backups";

/// The suffix of the world's mod data folder beside its save.
const DATA_SUFFIX: &str = "-mod-data";

/// How long the copy waits for the hold to take effect: the game sees it
/// after its next step.
const SETTLE: Duration = Duration::from_secs(1);

/// How often a copy starts again when the save changed under it.
const TRIES: u32 = 3;

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
    /// How many backups to keep.
    pub kept: usize,
}

/// Starts a backup: the saves are held now, and the copy runs once the hold
/// took effect; the reply says it started, and the caller reads how it
/// ended.
fn backup(tools: &Tools, request: &Request<'_>) -> Outcome {
    let Some(plan) = tools.backup_plan() else {
        return Outcome::refused(request.say(texts::REPLY_BACKUP_NONE, &[]));
    };
    if tools.backing_up.swap(true, Ordering::AcqRel) {
        return Outcome::refused(request.say(texts::REPLY_BACKUP_BUSY, &[]));
    }
    if let Err(why) = tools.world.hold_saves(true) {
        tools.backing_up.store(false, Ordering::Release);
        return Tools::failed(request, ids::BACKUP, &why);
    }
    let (Some(me), key) = (tools.me.upgrade(), request.player.key) else {
        return Outcome::refused(request.say(texts::REPLY_BACKUP_NONE, &[]));
    };
    tools
        .scheduler
        .after(SETTLE, move || Arc::clone(&me).copy_aside(&plan, key));
    Outcome::done(request.say(texts::REPLY_BACKUP_STARTED, &[]))
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
        let root = if settings.backup_dir.is_empty() {
            data.join(FOLDER)
        } else {
            mod_data.parent()?.join(&settings.backup_dir)
        };
        Some(Plan {
            saves: mod_data.parent()?.to_path_buf(),
            stem,
            root,
            kept: usize::try_from(settings.backups_kept).unwrap_or(usize::MAX),
        })
    }

    /// Copies the world aside on a thread of its own, lets the saves go and
    /// tells the account `key` how it went.
    fn copy_aside(self: Arc<Self>, plan: &Plan, key: PlayerKey) {
        let (tools, copying) = (Arc::clone(&self), plan.clone());
        let spawned = std::thread::Builder::new()
            .name("admin-tools backup".into())
            .spawn(move || {
                let copied = tools.copy_held(&copying);
                tools.finish_backup(&copying, copied, key);
            });
        if let Err(why) = spawned {
            self.finish_backup(plan, Err(why), key);
        }
    }

    /// Copies the world while its saves stay held, again when the save on
    /// disk changed during the copy; the backup's folder.
    fn copy_held(&self, plan: &Plan) -> io::Result<PathBuf> {
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

    /// Lets the saves go, prunes old backups and reports the result.
    fn finish_backup(&self, plan: &Plan, copied: io::Result<PathBuf>, key: PlayerKey) {
        if let Err(why) = self.world.hold_saves(false) {
            enshwick::warn!("backup: saves not let go: {why}");
        }
        let (line, outcome) = match copied {
            Ok(into) => {
                if let Err(why) = prune(&plan.root, plan.kept) {
                    enshwick::warn!("backup: old backups not removed: {why}");
                }
                let path = into.display().to_string();
                enshwick::info!("backup written to {path}");
                (
                    self.say(texts::NOTICE_BACKUP_DONE, &[(texts::value::PATH, &path)]),
                    path,
                )
            }
            Err(why) => {
                let why = why.to_string();
                enshwick::warn!("backup failed: {why}");
                (
                    self.say(texts::NOTICE_BACKUP_FAILED, &[(texts::value::WHY, &why)]),
                    why,
                )
            }
        };
        self.log(&["server", "", ids::BACKUP, "", &outcome]);
        self.backing_up.store(false, Ordering::Release);
        if key.0 == 0 {
            return;
        }
        let world = self.world;
        self.scheduler.next_tick(move || {
            if let Some(player) = world.player_by_key(key) {
                let _shown = world.notify(&player, &line);
            }
        });
    }
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

/// Whether the entry `name` in the saves' folder belongs to the world
/// `stem`: its save and the folders beside it that start with its key.
fn belongs(name: &str, stem: &str) -> bool {
    name.strip_prefix(stem)
        .is_some_and(|rest| rest.is_empty() || rest.starts_with(['.', '-', '_']))
}

/// Copies the world's files of `plan` into a new folder `name` under its
/// root, leaving the root out where it lies inside them; the new folder.
pub(crate) fn copy(plan: &Plan, name: &str) -> io::Result<PathBuf> {
    fs::create_dir_all(&plan.root)?;
    let into = plan.root.join(name);
    fs::create_dir(&into)?;
    let root = fs::canonicalize(&plan.root)?;
    for entry in fs::read_dir(&plan.saves)? {
        let entry = entry?;
        let name = entry.file_name();
        if name.to_str().is_some_and(|name| belongs(name, &plan.stem)) {
            copy_tree(&entry.path(), &into.join(&name), &root)?;
        }
    }
    Ok(into)
}

/// Copies `from` to `to`, a folder with all it holds, leaving out `skip`.
fn copy_tree(from: &Path, to: &Path, skip: &Path) -> io::Result<()> {
    let kind = fs::symlink_metadata(from)?.file_type();
    if kind.is_symlink() {
        return Ok(());
    }
    if !kind.is_dir() {
        return fs::copy(from, to).map(drop);
    }
    if fs::canonicalize(from)? == skip {
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
    use super::*;

    fn scratch(name: &str) -> PathBuf {
        let dir =
            std::env::temp_dir().join(format!("admin-tools-backup-{name}-{}", std::process::id()));
        let _gone = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();
        dir
    }

    #[test]
    fn a_world_s_files_start_with_its_key_and_another_world_s_do_not() {
        assert!(belongs("world0", "world0"));
        assert!(belongs("world0.ksd", "world0"));
        assert!(belongs("world0-mod-data", "world0"));
        assert!(belongs("world0_info", "world0"));
        assert!(!belongs("world01.ksd", "world0"));
        assert!(!belongs("backup", "world0"));
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
    fn a_second_backup_in_the_same_second_is_refused_rather_than_mixed() {
        let saves = scratch("twice");
        fs::write(saves.join("w.ksd"), "save").unwrap();
        let plan = Plan {
            saves: saves.clone(),
            stem: "w".into(),
            root: saves.join("out"),
            kept: 1,
        };
        copy(&plan, "2026-01-01_00-00-00").unwrap();
        assert!(copy(&plan, "2026-01-01_00-00-00").is_err());
        fs::remove_dir_all(&saves).unwrap();
    }
}
