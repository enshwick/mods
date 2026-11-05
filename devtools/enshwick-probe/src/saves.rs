//! The probe's checks of the world's save: what is on disk, `WorldSaving`,
//! and a save held and let go.

use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Duration;

use enshwick::events::WorldSaving;
use enshwick::schedule::Scheduler;
use enshwick::world::World;
use enshwick::{Load, Off};

/// How long after the other checks the hold starts, so the save they ask
/// for is on disk first.
const HOLD_AFTER: Duration = Duration::from_secs(30);
/// How long a hold takes to reach the game: it applies after the next step.
const HOLD_SETTLE: Duration = Duration::from_secs(1);

/// The save checks and what they need.
#[derive(Debug)]
pub(crate) struct SaveChecks {
    world: World,
    scheduler: Scheduler,
}

impl SaveChecks {
    /// Logs the first `WorldSaving` as a `PASS`.
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
        })?;
        Ok(Self {
            world: load.world()?,
            scheduler,
        })
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
