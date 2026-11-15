//! The platform's writes into the running game: a code patch, a hook inside
//! a function, a transaction of the world and an inventory batch, and the
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

/// The system whose first bytes the probe patches with the bytes already
/// there.
pub(super) const PATCHED: &str = "lifetime";
/// How many bytes of its entry the patch covers.
pub(super) const PATCH_LEN: usize = 8;
/// The system at whose entry the probe's hook inside a function counts hits.
pub(super) const COUNTED: &str = "last_transform";
/// The prop the transaction places: a wooden chest, a container.
pub(super) const CHEST: &str = "Prop_Storage_32_T2_Chest_Wood";
/// Metres above the first entity where the chest stands, in the air.
const CHEST_LIFT: f64 = 6.0;
/// How often the probe looks at the transaction again.
const LOOK: Duration = Duration::from_millis(250);
/// How many looks a transaction or its undo may take to settle.
const LOOKS: u32 = 40;

/// The probe's code patch: where, and the bytes it wrote back.
#[derive(Debug, Clone, Copy)]
struct Patch {
    at: usize,
    bytes: [u8; PATCH_LEN],
}

/// Asks for a patch of [`PATCHED`]'s first bytes with the same bytes, so
/// the game runs as before while the loader writes them.
fn patch(load: &mut Load<'_>) -> Result<Patch, Off> {
    let target = load.system(PATCHED)?;
    let code = load.code()?;
    let outside = || {
        Off::new(format!(
            "{PATCHED} at {target:#x} is not in the code section"
        ))
    };
    let at = usize::try_from(target).map_err(|_| outside())?;
    let bytes = at
        .checked_sub(code.as_ptr().addr())
        .and_then(|from| code.get(from..from.checked_add(PATCH_LEN)?))
        .and_then(|bytes| <[u8; PATCH_LEN]>::try_from(bytes).ok())
        .ok_or_else(outside)?;
    // SAFETY: the replacement is the bytes already at `target`, so every
    // path through them runs as before.
    unsafe { load.patch_code(target, &bytes, &bytes) }?;
    Ok(Patch { at, bytes })
}

/// Reads the patched bytes back from the running game's code.
fn read_back(patch: Patch) {
    // SAFETY: `patch.at` starts `PATCH_LEN` bytes inside the game's code
    // section, as `patch` checked against the loader's code range, which
    // stays mapped and readable for the process.
    let now: [u8; PATCH_LEN] = unsafe { read_at(core::ptr::with_exposed_provenance(patch.at), 0) };
    if now == patch.bytes {
        pass(&format!(
            "code patch: {PATCHED} reads its {PATCH_LEN} bytes back"
        ));
    } else {
        fail(&format!(
            "code patch: {PATCHED} reads {now:02x?}, wrote {:02x?}",
            patch.bytes
        ));
    }
}

/// Hooks [`COUNTED`]'s entry from inside the function, counting each hit.
fn hook_inside(load: &mut Load<'_>) -> Result<Arc<AtomicUsize>, Off> {
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

/// Logs how often the hook inside [`COUNTED`] ran in [`COUNT_FOR`].
fn report_hits(hits: usize) {
    if hits > 0 {
        pass(&format!(
            "hook inside {COUNTED}: {hits} hits in {COUNT_FOR:?}"
        ));
    } else {
        fail(&format!("hook inside {COUNTED}: no hit in {COUNT_FOR:?}"));
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

/// What a look at the chest's placement finds.
#[derive(Debug, PartialEq, Eq)]
enum Placement {
    /// The game has not placed it yet.
    Wait,
    /// It stands in the world, as this entity.
    Stands(Entity),
    /// It will not stand: the transaction ended otherwise, or took too long.
    Lost,
}

/// What look number `looks` at a transaction at `status`, listing `chest`,
/// finds. A prop listed before its transaction is applied is not in the
/// world yet.
fn placement(status: Option<&TransactionStatus>, chest: Option<Entity>, looks: u32) -> Placement {
    match (status, chest) {
        (Some(TransactionStatus::Applied), Some(chest)) => Placement::Stands(chest),
        (Some(TransactionStatus::Pending | TransactionStatus::Applied), _) if looks < LOOKS => {
            Placement::Wait
        }
        _ => Placement::Lost,
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

/// The parts the world writes need.
#[derive(Debug, Clone, Copy)]
struct Writer {
    world: World,
    inventories: Inventories,
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
        let commit = self
            .world
            .transaction()
            .place_prop(self.chest_item, WorldTransform::at([x, y + CHEST_LIFT, z]))
            .commit();
        let id = match commit {
            Ok(id) => id,
            Err(why) => {
                fail(&format!("transaction commit: {why}"));
                return;
            }
        };
        pass(&format!("transaction commit: {}", id.get()));
        let mut looks = 0;
        self.scheduler.every(LOOK, move || {
            looks += 1;
            let status = self.world.transaction_status(id);
            let chest = self.world.transaction_entities(id).first().copied();
            match placement(status.as_ref(), chest, looks) {
                Placement::Wait => return ControlFlow::Continue(()),
                Placement::Stands(chest) => {
                    pass(&format!(
                        "transaction status Applied, its entities name chest {}",
                        chest.0
                    ));
                    self.give_and_take(id, chest);
                }
                Placement::Lost => {
                    fail(&format!(
                        "transaction status {status:?}, its entities {chest:?}, after {looks} looks"
                    ));
                    self.undo(id);
                }
            }
            ControlFlow::Break(())
        });
    }

    /// Gives one item to `chest` and takes it back, each as an inventory
    /// batch, then undoes the transaction `id` that placed it.
    fn give_and_take(self, id: TransactionId, chest: Entity) {
        let give_one = Batch::new().with(Op::Give {
            to: chest,
            item: self.chest_item,
            count: 1,
            fill: None,
        });
        let _ticket = self.inventories.submit(&give_one, move |gave| {
            if gave.done == Done::TimedOut {
                enshwick::info!(
                    "SKIP inventory submit: the game ran no inventory step in time, as with nobody online"
                );
                self.undo(id);
                return;
            }
            let (gave, held) = (gave.is_committed(), self.inventories.count(chest, self.chest_item));
            let take_one = Batch::new().with(Op::Take {
                from: chest,
                item: self.chest_item,
                count: 1,
            });
            let _ticket = self.inventories.submit(&take_one, move |took| {
                let (took, left) = (took.is_committed(), self.inventories.count(chest, self.chest_item));
                let line = format!(
                    "inventory submit: give committed {gave}, chest held {held}; take committed {took}, chest holds {left}"
                );
                if gave && held == 1 && took && left == 0 {
                    pass(&line);
                } else {
                    fail(&line);
                }
                self.undo(id);
            });
        });
    }

    /// Undoes the transaction `id` and follows its status until it settles.
    fn undo(self, id: TransactionId) {
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
                Some(true) => pass("transaction undo: status Undone"),
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
    patch: Option<Patch>,
    hits: Option<Arc<AtomicUsize>>,
    writer: Option<Writer>,
    scheduler: Scheduler,
}

impl WriteChecks {
    /// Asks for the patch and the hook, which only init may, and sets up
    /// the world writes.
    pub(super) fn new(load: &mut Load<'_>, scheduler: Scheduler, world: Option<World>) -> Self {
        let patch = part("code patch", patch(load));
        let hits = part("hook inside a function", hook_inside(load));
        let inventories = part("inventories", load.inventories());
        let chest = part("chest item", vanilla::item(CHEST)).map(ItemId::from_guid);
        let writer = world
            .zip(inventories)
            .zip(chest)
            .map(|((world, inventories), chest_item)| Writer {
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
            None => off("hook inside a function"),
        }
        match self.writer {
            Some(writer) => writer.place(),
            None => off("world writes"),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_chest_stands_once_applied_and_listed() {
        let chest = Entity(7);
        assert_eq!(
            placement(Some(&TransactionStatus::Applied), Some(chest), 1),
            Placement::Stands(chest)
        );
    }

    #[test]
    fn a_pending_or_unlisted_chest_waits_until_the_last_look() {
        assert_eq!(
            placement(Some(&TransactionStatus::Pending), None, 1),
            Placement::Wait
        );
        assert_eq!(
            placement(Some(&TransactionStatus::Applied), None, LOOKS - 1),
            Placement::Wait
        );
        assert_eq!(
            placement(Some(&TransactionStatus::Pending), None, LOOKS),
            Placement::Lost
        );
    }

    #[test]
    fn a_failed_or_forgotten_transaction_is_lost_at_once() {
        let failed = TransactionStatus::Failed(enshwick::WorldError::NotLoaded);
        assert_eq!(placement(Some(&failed), None, 1), Placement::Lost);
        assert_eq!(placement(None, None, 1), Placement::Lost);
    }

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
}
