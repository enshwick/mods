//! The check of the notifiers a mod raises: the probe raises one of the
//! game's notifiers far from every player and hears it back as the server's
//! notify event.

use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Duration;

use enshwick::content::Guid;
use enshwick::effects::{Effects, Notify, NotifyAt};
use enshwick::resources::Registry;
use enshwick::schedule::Scheduler;
use enshwick::typed::{NotifierResource, ResourceType};
use enshwick::{Load, Off};

use crate::checks::{fail, pass};

/// Where the probe raises its notifier, far from anything a player would
/// hear.
const NOTIFY_AT: [f64; 3] = [-12345.0, -500.0, 777.0];
/// How far from [`NOTIFY_AT`] the event heard back may name its place.
const NOTIFY_SLACK: f64 = 1.0;
/// How long the raised notifier may take to come back as an event.
const HEAR_WAIT: Duration = Duration::from_secs(2);

/// The notifier check and what it needs.
#[derive(Debug)]
pub(crate) struct NotifierChecks {
    effects: Effects,
    scheduler: Scheduler,
    notifier: Guid,
    heard: Arc<AtomicBool>,
}

impl NotifierChecks {
    /// Picks one of the game's notifiers and listens for its notify events,
    /// which only init may.
    pub(crate) fn new(load: &mut Load<'_>, scheduler: Scheduler) -> Result<Self, Off> {
        let notifier = a_notifier(load)?;
        let heard = Arc::new(AtomicBool::new(false));
        let listener = Arc::clone(&heard);
        load.on_notify(move |event| {
            let near = event
                .position
                .iter()
                .zip(NOTIFY_AT)
                .all(|(a, b)| (a - b).abs() <= NOTIFY_SLACK);
            if event.type_guid == notifier && near {
                listener.store(true, Ordering::Release);
            }
        })?;
        pass("notify handler registered");
        Ok(Self {
            effects: load.effects(),
            scheduler,
            notifier,
            heard,
        })
    }

    /// Raises the notifier and looks whether the server's notify event
    /// named it, on the game thread.
    pub(crate) fn run(&self) {
        self.heard.store(false, Ordering::Release);
        let notify = Notify::new(self.notifier, NotifyAt::Position(NOTIFY_AT));
        if let Err(why) = self.effects.notify(&notify) {
            fail(&format!("effects notify of {}: {why}", self.notifier));
            return;
        }
        let (heard, notifier) = (Arc::clone(&self.heard), self.notifier);
        self.scheduler.after(HEAR_WAIT, move || {
            if heard.load(Ordering::Acquire) {
                pass(&format!(
                    "effects notify: notifier {notifier} raised and heard back"
                ));
            } else {
                fail(&format!(
                    "effects notify: notifier {notifier} raised, no notify event within {HEAR_WAIT:?}"
                ));
            }
        });
    }
}

/// The first notifier the game's archive holds.
fn a_notifier(load: &Load<'_>) -> Result<Guid, Off> {
    let ty = ResourceType::<NotifierResource>::resolve(load)?;
    let hash = ty.key(Guid([0; 16])).type_hash;
    let host = enshwick::host().ok_or_else(|| Off::new("no loader"))?;
    host.resource_keys(Registry::game()?.hashed(hash))?
        .into_iter()
        .find(|key| key.type_hash == hash)
        .map(|key| key.guid)
        .ok_or_else(|| Off::new("the game has no notifier"))
}
