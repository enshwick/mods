//! The probe's checks of the world's save: what is on disk, `WorldSaving`,
//! and a save held and let go.

use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Duration;

use enshwick::content::Guid;
use enshwick::events::{WorldSaved, WorldSaving};
use enshwick::schedule::Scheduler;
use enshwick::world::{SaveInfo, World};
use enshwick::{Load, Off};

/// How long after the other checks the hold starts, so the save they ask
/// for is on disk first.
const HOLD_AFTER: Duration = Duration::from_secs(30);
/// How long a hold takes to reach the game: it applies after the next step.
const HOLD_SETTLE: Duration = Duration::from_secs(1);

/// The line a `WorldSaved` logs: the world and when its save on disk was
/// written.
fn saved(info: Option<SaveInfo>) -> String {
    match info {
        Some(info) => format!(
            "WorldSaved: world {}, saved {} s after the epoch, {} bytes",
            Guid(info.world.0),
            info.saved_at.as_secs(),
            info.size
        ),
        None => "WorldSaved: the save on disk does not read".to_owned(),
    }
}

/// The save checks and what they need.
#[derive(Debug)]
pub(crate) struct SaveChecks {
    world: World,
    scheduler: Scheduler,
}

impl SaveChecks {
    /// Logs the first `WorldSaving` as a `PASS`, and every `WorldSaving`
    /// and `WorldSaved`, the latter with the save on disk, so a save a
    /// player makes in a hosted game shows in order.
    ///
    /// # Errors
    ///
    /// [`Off`] without the world or its events.
    pub(crate) fn new(load: &mut Load<'_>, scheduler: Scheduler) -> Result<Self, Off> {
        let seen = AtomicBool::new(false);
        load.on::<WorldSaving>(move |()| {
            if !seen.swap(true, Ordering::Relaxed) {
                enshwick::info!("PASS WorldSaving raised as a save starts");
            }
            enshwick::info!("WorldSaving");
        })?;
        let world = load.world()?;
        load.on::<WorldSaved>(move |()| enshwick::info!("{}", saved(world.save_info())))?;
        Ok(Self { world, scheduler })
    }

    /// Reads the save on disk, then holds saves, asks for one that must be
    /// refused, and lets them go.
    pub(crate) fn run(&self) {
        match self.world.save_info() {
            Some(info) => enshwick::info!(
                "PASS save info: saved {} s after the epoch, {} bytes, {} backups",
                info.saved_at.as_secs(),
                info.size,
                info.backups
            ),
            None => enshwick::info!("SKIP save info: no save on disk yet"),
        }
        let world = self.world;
        let scheduler = self.scheduler;
        self.scheduler.after(HOLD_AFTER, move || {
            if let Err(why) = world.hold_saves(true) {
                enshwick::warn!("FAIL saves held: {why}");
                return;
            }
            enshwick::info!("PASS saves held");
            scheduler.after(HOLD_SETTLE, move || {
                match world.save() {
                    Err(why) => enshwick::info!("PASS a save while held is refused: {why}"),
                    Ok(()) => enshwick::warn!("FAIL a save while held was asked"),
                }
                match world.hold_saves(false) {
                    Ok(()) => enshwick::info!("PASS saves let go"),
                    Err(why) => enshwick::warn!("FAIL saves let go: {why}"),
                }
            });
        });
    }
}

#[cfg(test)]
mod tests {
    use enshwick::abi::{Header, Side, Status};
    use enshwick::{Load, Mod, Off};

    use super::*;

    /// The save checks alone.
    struct Saves;

    impl Mod for Saves {
        fn init(load: &mut Load<'_>) -> Result<Self, Off> {
            let scheduler = load.scheduler()?;
            SaveChecks::new(load, scheduler).map(|_| Self)
        }
    }

    enshwick::test::declare_mod!(
        Saves,
        r#"
        id = "enshwick.probe"
        side = "server"
        "#
    );

    fn lines(loader: &enshwick::test::Installed) -> Vec<String> {
        loader.logs().into_iter().map(|(_, line)| line).collect()
    }

    #[test]
    fn a_hosting_client_logs_each_save_s_start_then_its_end_with_the_world() {
        let loader = enshwick::test::FakeHost::new()
            .world()
            .side(Side::CLIENT)
            .hosting(true)
            .install();
        // SAFETY: the exports of this module's `declare_mod!(Saves)`.
        let status =
            unsafe { loader.start(enshwick_mod_query, enshwick_mod_init, enshwick_mod_ready) };
        assert_eq!(status, Status::OK, "{:?}", loader.faults());
        loader.set_save_info(Some(enshwick::abi::SaveInfo {
            header: Header::of::<enshwick::abi::SaveInfo>(),
            world: [1; 16],
            saved_at: 60,
            size: 10,
            backups: 0,
            reserved: 0,
        }));
        for _ in 0..2 {
            let _handled = loader.raise::<WorldSaving>(&());
            let _handled = loader.raise::<WorldSaved>(&());
        }
        let world = Guid([1; 16]);
        let saved = format!("WorldSaved: world {world}, saved 60 s after the epoch, 10 bytes");
        let ours: Vec<String> = lines(&loader)
            .into_iter()
            .filter(|line| line.starts_with("World") || line.starts_with("PASS"))
            .collect();
        assert_eq!(
            ours,
            [
                "PASS WorldSaving raised as a save starts",
                "WorldSaving",
                &saved,
                "WorldSaving",
                &saved,
            ]
        );
    }

    #[test]
    fn a_save_that_does_not_read_is_said_so() {
        assert_eq!(saved(None), "WorldSaved: the save on disk does not read");
    }
}
