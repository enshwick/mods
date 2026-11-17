//! The platform's writes into the running game: a code patch, a hook at an
//! instruction, a transaction of the world and an inventory batch, and the
//! server world they act in.

use core::ops::ControlFlow;
use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::time::Duration;

use enshwick::content::vanilla;
use enshwick::inventory::{Batch, Done, Inventories, Op};
use enshwick::raw::read_at;
use enshwick::schedule::Scheduler;
use enshwick::world::{TransactionId, TransactionStatus, World, WorldTransform};
use enshwick::{Entity, ItemId, Load, Off};

use super::{COUNT_FOR, off, part};
use crate::checks::{fail, pass};

/// How many bytes the patch covers.
pub(super) const PATCH_LEN: usize = 8;
/// The `int3` bytes the compiler fills the gaps between the game's
/// functions with, which no code runs; the patch goes over a run of them.
pub(super) const PADDING: u8 = 0xcc;
/// What the patch writes: `nop`s, which differ from the padding, so reading
/// them back shows the loader wrote the patch.
pub(super) const WRITTEN: [u8; PATCH_LEN] = [0x90; PATCH_LEN];
/// The system at whose entry the probe's hook at an instruction counts hits.
pub(super) const COUNTED: &str = "last_transform";
/// The prop the transaction places: a wooden chest, a container.
pub(super) const CHEST: &str = "Prop_Storage_32_T2_Chest_Wood";
/// Metres above the first entity where the chest stands, in the air.
const CHEST_LIFT: f64 = 6.0;
/// How long the chest's placement gets to stand.
const PLACING: Duration = Duration::from_secs(10);
/// How often the probe looks at an undo again.
const LOOK: Duration = Duration::from_millis(250);
/// How many looks an undo may take to settle.
const LOOKS: u32 = 40;

/// Where in the code section the first run of [`PATCH_LEN`] padding bytes
/// starts.
fn padding(code: &[u8]) -> Option<usize> {
    code.windows(PATCH_LEN)
        .position(|run| run.iter().all(|&byte| byte == PADDING))
}

/// Asks for a patch that writes [`WRITTEN`] over the code section's first
/// run of padding, so the game runs as before while the bytes change.
fn patch(load: &mut Load<'_>) -> Result<usize, Off> {
    let code = load.code()?;
    let from = padding(code).ok_or_else(|| {
        Off::new(format!(
            "no {PATCH_LEN} bytes of padding in the code section"
        ))
    })?;
    let at = code.as_ptr().addr() + from;
    let target = u64::try_from(at).map_err(|_| Off::new(format!("padding at {at:#x}")))?;
    // SAFETY: the bytes at `target` are padding between functions, which
    // no path through the game's code runs or jumps into.
    unsafe { load.patch_code(target, &[PADDING; PATCH_LEN], &WRITTEN) }?;
    Ok(at)
}

/// Reads the patched bytes at `at` back from the running game's code.
fn read_back(at: usize) {
    // SAFETY: `at` starts `PATCH_LEN` bytes inside the game's code section,
    // which stays mapped and readable for the process. The loader writes the
    // patch while it arms, before this check runs, since no thread runs
    // padding to make it wait, and nothing writes those bytes afterwards.
    let now: [u8; PATCH_LEN] = unsafe { read_at(core::ptr::with_exposed_provenance(at), 0) };
    if now == WRITTEN {
        pass(&format!(
            "code patch: padding at {at:#x} reads the {PATCH_LEN} bytes written"
        ));
    } else {
        fail(&format!(
            "code patch: padding at {at:#x} reads {now:02x?}, not {WRITTEN:02x?}"
        ));
    }
}

/// Hooks the instruction at [`COUNTED`]'s entry, counting each hit.
fn hook_at(load: &mut Load<'_>) -> Result<Arc<AtomicUsize>, Off> {
    let at = load.system(COUNTED)?;
    let hits = Arc::new(AtomicUsize::new(0));
    let counted = Arc::clone(&hits);
    // SAFETY: `at` is the entry of an ECS system, the first byte of its
    // first instruction; no code jumps into its first 5 bytes, since its
    // loop goes back to the iterator call, not the entry (decompiled on
    // server 26430253); the handler writes no register.
    unsafe {
        load.hook_at(at, move |_| {
            counted.fetch_add(1, Ordering::Relaxed);
        })
    }?;
    Ok(hits)
}

/// Logs how often the hook at [`COUNTED`]'s entry ran in [`COUNT_FOR`].
fn report_hits(hits: usize) {
    if hits > 0 {
        pass(&format!("hook at {COUNTED}: {hits} hits in {COUNT_FOR:?}"));
    } else {
        fail(&format!("hook at {COUNTED}: no hit in {COUNT_FOR:?}"));
    }
}

/// Asks the loader through its raw table for the server's entity world,
/// which no safe call hands a mod.
fn server_world() {
    let Some(host) = enshwick::host() else {
        return;
    };
    let mut world = 0u64;
    let status = (host.api().server_world)(host.handle(), &raw mut world);
    if status.is_ok() && world != 0 {
        pass("server world: set while the world runs");
    } else {
        fail(&format!("server world: {world:#x}, {status:?}"));
    }
}

/// Whether an undo at `status` on look number `looks` passed, failed, or
/// is still running (`None`).
fn undone(status: Option<&TransactionStatus>, looks: u32) -> Option<bool> {
    match status {
        Some(TransactionStatus::Undone) => Some(true),
        Some(TransactionStatus::Pending | TransactionStatus::Undoing) if looks < LOOKS => None,
        _ => Some(false),
    }
}

/// Whether a transaction at `status` may have left something in the world
/// that its undo takes back.
fn may_stand(status: Option<&TransactionStatus>) -> bool {
    matches!(
        status,
        Some(TransactionStatus::Pending | TransactionStatus::Applied)
    )
}

/// Whether `found` is within a metre of `spot`, where the chest was placed.
fn stands_at(found: Option<[f64; 3]>, spot: [f64; 3]) -> bool {
    found.is_some_and(|found| {
        found
            .iter()
            .zip(spot)
            .map(|(a, b)| (a - b).powi(2))
            .sum::<f64>()
            < 1.0
    })
}

/// The parts the world writes need; without inventories the transaction
/// still runs, and only the inventory batch is off.
#[derive(Debug, Clone, Copy)]
struct Writer {
    world: World,
    inventories: Option<Inventories>,
    scheduler: Scheduler,
    chest_item: ItemId,
}

impl Writer {
    /// Places a chest in the air above the first entity, then fills,
    /// empties and undoes it.
    fn place(self) {
        let Some([x, y, z]) = self
            .world
            .entities(self.world.transform_type())
            .first()
            .map(|placed| placed.position)
        else {
            enshwick::info!("SKIP transaction: no placed entity");
            return;
        };
        let spot = [x, y + CHEST_LIFT, z];
        let commit = self
            .world
            .transaction()
            .place_prop(self.chest_item, WorldTransform::at(spot))
            .commit();
        let id = match commit {
            Ok(id) => id,
            Err(why) => {
                fail(&format!("transaction commit: {why}"));
                return;
            }
        };
        pass(&format!("transaction commit: {}", id.get()));
        let waited = self.world.when_settled(id, PLACING, move |settled| {
            let Some(chest) = settled.standing() else {
                let (status, chest) = (settled.status, settled.props.first().copied());
                fail(&format!(
                    "transaction status {status:?}, its entities {chest:?}, once settled or after {PLACING:?}"
                ));
                if let Some(chest) = chest.filter(|_| may_stand(status.as_ref())) {
                    self.undo(id, chest);
                }
                return;
            };
            let found = self.world.position(chest);
            if stands_at(found, spot) {
                let status = settled.status.map_or_else(String::new, |s| format!("{s:?}"));
                pass(&format!(
                    "transaction status {status}, its entities name chest {} where it was placed",
                    chest.0
                ));
            } else {
                fail(&format!(
                    "transaction entities name {} at {found:?}, not the chest placed at {spot:?}",
                    chest.0
                ));
            }
            if let Some(inventories) = self.inventories {
                self.give_and_take(inventories, id, chest);
            } else {
                off("inventories");
                self.undo(id, chest);
            }
        });
        if let Err(why) = waited {
            fail(&format!("transaction wait: {why}"));
            if let Err(why) = self.world.undo(id) {
                fail(&format!("transaction undo after a refused wait: {why}"));
            }
        }
    }

    /// Gives one item to `chest` and takes it back, each as an inventory
    /// batch, then undoes the transaction `id` that placed it.
    fn give_and_take(self, inventories: Inventories, id: TransactionId, chest: Entity) {
        let give_one = Batch::new()
            .with(Op::Give {
                to: chest,
                item: self.chest_item,
                count: 1,
                fill: None,
            })
            .hide_in_ui();
        let _ticket = inventories.submit(&give_one, move |gave| {
            if gave.done == Done::TimedOut {
                enshwick::info!(
                    "SKIP inventory submit: the game ran no inventory step in time, as with nobody online"
                );
                self.undo(id, chest);
                return;
            }
            let (gave, held) = (gave.is_committed(), inventories.count(chest, self.chest_item));
            let take_one = Batch::new().with(Op::Take {
                from: chest,
                item: self.chest_item,
                count: 1,
            });
            let _ticket = inventories.submit(&take_one, move |took| {
                let (took, left) = (took.is_committed(), inventories.count(chest, self.chest_item));
                let line = format!(
                    "inventory submit: give hidden in the UI committed {gave}, chest held {held}; take committed {took}, chest holds {left}"
                );
                if gave && held == 1 && took && left == 0 {
                    pass(&line);
                } else {
                    fail(&line);
                }
                self.undo(id, chest);
            });
        });
    }

    /// Undoes the transaction `id`, follows its status until it settles and
    /// looks that `chest` left the world.
    fn undo(self, id: TransactionId, chest: Entity) {
        if let Err(why) = self.world.undo(id) {
            fail(&format!("transaction undo: {why}"));
            return;
        }
        let mut looks = 0;
        self.scheduler.every(LOOK, move || {
            looks += 1;
            let status = self.world.transaction_status(id);
            match undone(status.as_ref(), looks) {
                None => return ControlFlow::Continue(()),
                Some(true) => match self.world.position(chest) {
                    None => pass("transaction undo: status Undone, the chest gone"),
                    Some(at) => fail(&format!(
                        "transaction undo: status Undone, but chest {} still stands at {at:?}",
                        chest.0
                    )),
                },
                Some(false) => fail(&format!(
                    "transaction undo: status {status:?} after {looks} looks"
                )),
            }
            ControlFlow::Break(())
        });
    }
}

/// The checks of the platform's writes into the game, and the parts each
/// needs; a part the loader does not give turns off only its check.
#[derive(Debug)]
pub(super) struct WriteChecks {
    patch: Option<usize>,
    hits: Option<Arc<AtomicUsize>>,
    writer: Option<Writer>,
    scheduler: Scheduler,
}

impl WriteChecks {
    /// Asks for the patch and the hook, which only init may, and sets up
    /// the world writes.
    pub(super) fn new(load: &mut Load<'_>, scheduler: Scheduler, world: Option<World>) -> Self {
        let patch = part("code patch", patch(load));
        let hits = part("hook at an instruction", hook_at(load));
        let inventories = part("inventories", load.inventories());
        let chest = part("chest item", vanilla::item(CHEST)).map(ItemId::from_guid);
        let writer = world.zip(chest).map(|(world, chest_item)| Writer {
            world,
            inventories,
            scheduler,
            chest_item,
        });
        Self {
            patch,
            hits,
            writer,
            scheduler,
        }
    }

    /// Runs every check on the game thread.
    pub(super) fn run(&self) {
        server_world();
        match self.patch {
            Some(patch) => read_back(patch),
            None => off("code patch"),
        }
        match &self.hits {
            Some(hits) => {
                let (hits, before) = (Arc::clone(hits), hits.load(Ordering::Relaxed));
                self.scheduler.after(COUNT_FOR, move || {
                    report_hits(hits.load(Ordering::Relaxed) - before);
                });
            }
            None => off("hook at an instruction"),
        }
        match self.writer {
            Some(writer) => writer.place(),
            None => off("world writes"),
        }
    }

    /// How many hits the hook at [`COUNTED`]'s entry counted so far.
    #[cfg(test)]
    pub(super) fn hits(&self) -> Option<usize> {
        self.hits.as_ref().map(|hits| hits.load(Ordering::Relaxed))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn an_undo_passes_once_undone_and_fails_when_it_stalls_or_is_refused() {
        assert_eq!(undone(Some(&TransactionStatus::Undone), 1), Some(true));
        assert_eq!(undone(Some(&TransactionStatus::Undoing), 1), None);
        assert_eq!(
            undone(Some(&TransactionStatus::Undoing), LOOKS),
            Some(false)
        );
        let refused = TransactionStatus::UndoRefused(enshwick::WorldError::Refused);
        assert_eq!(undone(Some(&refused), 1), Some(false));
        assert_eq!(undone(None, 1), Some(false));
    }

    #[test]
    fn only_a_pending_or_applied_transaction_is_undone_after_a_loss() {
        assert!(may_stand(Some(&TransactionStatus::Pending)));
        assert!(may_stand(Some(&TransactionStatus::Applied)));
        let failed = TransactionStatus::Failed(enshwick::WorldError::NotLoaded);
        assert!(!may_stand(Some(&failed)));
        assert!(!may_stand(Some(&TransactionStatus::Expired)));
        assert!(!may_stand(None));
    }

    #[test]
    fn the_chest_stands_only_within_a_metre_of_its_spot() {
        let spot = [10.0, 20.0, 30.0];
        assert!(stands_at(Some([10.5, 20.0, 30.5]), spot));
        assert!(!stands_at(Some([11.0, 20.0, 30.0]), spot));
        assert!(!stands_at(None, spot));
    }

    #[test]
    fn the_patch_takes_the_first_run_of_padding_long_enough() {
        let mut code = [0x90; 32];
        code[2..2 + PATCH_LEN - 1].fill(PADDING);
        code[20..20 + PATCH_LEN].fill(PADDING);
        assert_eq!(padding(&code), Some(20));
        assert_eq!(padding(&[PADDING; PATCH_LEN - 1]), None);
    }
}
