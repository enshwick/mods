//! Who built, removed or dug where: a log in the world's data folder,
//! written while the world's settings turn it on, and the lookup of what
//! changed near a spot.

use std::sync::Arc;
use std::time::{Duration, Instant};

use enshwick::abi::BuildingData;
use enshwick::actions::{Action, Arg, Outcome, Request, Value, Via};
use enshwick::events::{BuildingPlaced, BuildingRemoved};
use enshwick::voxels::{EditSource, PER_METRE, VoxelEdit};
use enshwick::{Load, Off, PlayerKey, Relock as _};

use crate::{Tools, ids, journal, texts};

/// The build log: one line per building placed or removed and per spot of
/// terrain changed.
pub(crate) const LOG: journal::Files = journal::Files {
    now: "builds.log",
    old: "builds.1.log",
};

/// What a line of the log says happened: its word in the log and the key
/// of the text players read.
const KINDS: [(&str, &str); 5] = [
    (PLACED, texts::BUILD_PLACED),
    (PICKED_UP, texts::BUILD_PICKED_UP),
    (TORN_DOWN, texts::BUILD_TORN_DOWN),
    (DUG, texts::BUILD_DUG),
    (FILLED, texts::BUILD_FILLED),
];

const PLACED: &str = "placed";
const PICKED_UP: &str = "picked-up";
const TORN_DOWN: &str = "torn-down";
const DUG: &str = "dug";
const FILLED: &str = "filled";

/// How far from changed terrain the nearest player may stand to be named
/// as who changed it, in metres: the event names no player.
const REACH: f64 = 16.0;

/// The side of the cells terrain changes are told apart by, in metres: a
/// player digging on in one cell writes one line, not one per stroke.
const CELL: f64 = 8.0;

/// How long a player's changes of one kind in one cell stay one line.
const SAME_SPOT: Duration = Duration::from_secs(30);

/// The widest lookup, in metres.
const MAX_RADIUS: f64 = 200.0;

/// The most lines a lookup reads from the log's end.
const MAX_READ: usize = 2_000;

/// The most changes a lookup answers with.
pub(crate) const MAX_FOUND: usize = 10;

/// A change the log keeps.
#[derive(Debug, Clone, PartialEq)]
pub(crate) struct Change {
    /// When, as the log writes it, UTC.
    pub time: String,
    /// The player, as the audit log names accounts.
    pub who: String,
    /// What happened, one of the words of [`KINDS`].
    pub kind: String,
    /// Where, in whole metres.
    pub at: [f64; 3],
}

impl Change {
    /// The change a line of the log holds; `None` for any other line.
    pub(crate) fn parse(line: &str) -> Option<Self> {
        let mut fields = line.split('\t');
        let (time, who, kind) = (fields.next()?, fields.next()?, fields.next()?);
        let mut axis = || {
            fields
                .next()?
                .parse::<f64>()
                .ok()
                .filter(|at| at.is_finite())
        };
        let at = [axis()?, axis()?, axis()?];
        Some(Self {
            time: time.to_owned(),
            who: who.to_owned(),
            kind: kind.to_owned(),
            at,
        })
    }

    /// The key of the text for what happened; the word itself for one the
    /// mod does not know.
    pub(crate) fn kind_text(&self) -> &str {
        KINDS
            .iter()
            .find(|(word, _)| *word == self.kind)
            .map_or(self.kind.as_str(), |(_, key)| key)
    }
}

/// The changes of `lines`, oldest first, that lie within `radius` metres
/// of `centre`: the newest first, at most [`MAX_FOUND`].
pub(crate) fn near(lines: &[String], centre: [f64; 3], radius: f64) -> Vec<Change> {
    lines
        .iter()
        .rev()
        .filter_map(|line| Change::parse(line))
        .filter(|change| distance(change.at, centre) <= radius)
        .take(MAX_FOUND)
        .collect()
}

fn distance(a: [f64; 3], b: [f64; 3]) -> f64 {
    (0..3).map(|i| (a[i] - b[i]).powi(2)).sum::<f64>().sqrt()
}

/// The last terrain change written for each player, so changes in one
/// spot stay one line.
#[derive(Debug, Default)]
pub(crate) struct Spots(Vec<(PlayerKey, &'static str, [i64; 3], Instant)>);

impl Spots {
    /// Whether the change `kind` by `who` at `at` is new: not the spot and
    /// kind written for them within [`SAME_SPOT`] before `now`. A new one
    /// is remembered in place of their last.
    fn is_new(&mut self, who: PlayerKey, kind: &'static str, at: [f64; 3], now: Instant) -> bool {
        #[expect(
            clippy::cast_possible_truncation,
            reason = "a cell index of a position inside the world"
        )]
        let cell = at.map(|axis| (axis / CELL).floor() as i64);
        let last = self.0.iter().position(|(key, ..)| *key == who);
        if let Some(at) = last {
            let (_, last_kind, last_cell, when) = self.0[at];
            if last_kind == kind && last_cell == cell && now.duration_since(when) < SAME_SPOT {
                return false;
            }
            self.0.swap_remove(at);
        }
        self.0.push((who, kind, cell, now));
        true
    }

    /// Forgets every spot, as when the game ends.
    pub(crate) fn clear(&mut self) {
        self.0.clear();
    }
}

/// Declares the lookup and listens for buildings and terrain changes.
pub(crate) fn add(tools: &Arc<Tools>, load: &mut Load<'_>) -> Result<(), Off> {
    tools.add(
        Action::new(ids::WHO, texts::ACTION_WHO)
            .help(texts::ACTION_WHO_HELP)
            .arg(Arg::number(texts::ARG_RADIUS, 1.0..=MAX_RADIUS, 1.0)),
        who,
    )?;
    let placed = Arc::clone(tools);
    load.on::<BuildingPlaced>(move |data| placed.built(data))?;
    let removed = Arc::clone(tools);
    load.on::<BuildingRemoved>(move |data| removed.built(data))?;
    let dug = Arc::clone(tools);
    // A build without the terrain editor's write still logs buildings.
    if let Err(why) = load.on_voxels_changed(move |edit| dug.shaped(edit)) {
        enshwick::warn!("the build log leaves terrain out: {why}");
    }
    Ok(())
}

/// What changed within the radius of the caller, newest first.
fn who(tools: &Tools, request: &Request<'_>) -> Outcome {
    let [Value::Number(radius)] = request.args else {
        return Outcome::refused(request.say(texts::REPLY_LOG_NONE, &[]));
    };
    if !tools.settings.load().build_log {
        return Outcome::refused(request.say(texts::REPLY_BUILDS_OFF, &[]));
    }
    let Some(centre) = tools.world.position(request.player.entity) else {
        return Outcome::refused(request.say(texts::REPLY_NO_CHARACTER, &[]));
    };
    let Some(dir) = tools.world.data_dir() else {
        return Outcome::refused(request.say(texts::REPLY_LOG_NONE, &[]));
    };
    let found = match LOG.tail(&dir, MAX_READ) {
        Ok(lines) => near(&lines, centre, *radius),
        Err(why) => return Tools::failed(request, LOG.now, &why),
    };
    let radius = format!("{radius:.0}");
    if found.is_empty() {
        return Outcome::done(
            request.say(texts::REPLY_WHO_NONE, &[(texts::value::RADIUS, &radius)]),
        );
    }
    let count = found.len().to_string();
    let head = request.say(
        texts::REPLY_WHO,
        &[
            (texts::value::COUNT, &count),
            (texts::value::RADIUS, &radius),
        ],
    );
    if matches!(request.via, Via::Interface) {
        // The interface's notice holds one line: the changes go in a window.
        return match tools.show_changes(request, &found) {
            Ok(()) => Outcome::done(head),
            Err(why) => Tools::failed(request, ids::WHO, &why),
        };
    }
    let rows = found.iter().map(|change| {
        let kind = request.say(change.kind_text(), &[]);
        request.say(
            texts::REPLY_WHO_ROW,
            &crate::players::borrowed(&row(change, kind)),
        )
    });
    Outcome::done(
        core::iter::once(head)
            .chain(rows)
            .collect::<Vec<_>>()
            .join(crate::LINE),
    )
}

/// A change as a line of a lookup reads it: the values of
/// [`texts::REPLY_WHO_ROW`], `kind` being what happened in the reader's
/// language.
pub(crate) fn row(change: &Change, kind: String) -> [(&'static str, String); 6] {
    let [x, y, z] = change.at.map(|axis| format!("{axis:.0}"));
    [
        (texts::value::TIME, change.time.clone()),
        (texts::value::NAME, change.who.clone()),
        (texts::value::KIND, kind),
        (texts::value::X, x),
        (texts::value::Y, y),
        (texts::value::Z, z),
    ]
}

impl Tools {
    /// Writes a building placed, picked up or torn down to the build log
    /// while the world's settings keep it on. A removal says no place, so
    /// it is written where its player stands, and not at all without one,
    /// since a lookup finds a change only by its place.
    fn built(&self, data: &BuildingData) {
        if !self.settings.load().build_log {
            return;
        }
        let kind = match data.kind {
            BuildingData::PLACED => PLACED,
            BuildingData::PICKED_UP => PICKED_UP,
            _ => TORN_DOWN,
        };
        let player = self.world.player(data.player);
        let at = if data.kind == BuildingData::PLACED {
            Some(data.position.map(f64::from))
        } else {
            self.world.position(data.player)
        };
        let Some(at) = at else {
            return;
        };
        let who = player.map(|player| crate::account(&player.name, player.key));
        self.write_change(who.as_deref(), kind, at);
    }

    /// Writes terrain the game changed to the build log while the world's
    /// settings keep it on, naming the player nearest to it within
    /// [`REACH`]: the game says where, not who.
    fn shaped(&self, edit: &VoxelEdit) {
        if edit.source != EditSource::Game || !self.settings.load().build_log {
            return;
        }
        let dug = edit.changes.iter().filter(|c| c.after.is_air()).count();
        let kind = if dug * 2 >= edit.changes.len() {
            DUG
        } else {
            FILLED
        };
        let low = edit.min.to_metres();
        let at = [0, 1, 2].map(|i| low[i] + f64::from(edit.size[i]) / PER_METRE / 2.0);
        let nearest = self
            .world
            .players()
            .into_iter()
            .filter_map(|player| {
                let far = distance(self.world.position(player.entity)?, at);
                (far <= REACH).then_some((far, player))
            })
            .min_by(|a, b| a.0.total_cmp(&b.0))
            .map(|(_, player)| player);
        let key = nearest.as_ref().map_or(PlayerKey::new(0), |p| p.key);
        if !self.spots.relock().is_new(key, kind, at, Instant::now()) {
            return;
        }
        let who = nearest.map(|player| crate::account(&player.name, player.key));
        self.write_change(who.as_deref(), kind, at);
    }

    /// Appends a change to the build log: `who`, or a dash for nobody near.
    fn write_change(&self, who: Option<&str>, kind: &str, at: [f64; 3]) {
        let Some(dir) = self.world.data_dir() else {
            return;
        };
        let [x, y, z] = at.map(|axis| format!("{axis:.0}"));
        self.builds
            .record(dir, &[who.unwrap_or("-"), kind, &x, &y, &z]);
    }
}

#[cfg(test)]
mod tests {
    use proptest::prelude::*;

    use super::*;

    fn line(who: &str, kind: &str, at: [i32; 3]) -> String {
        format!(
            "2026-11-10 12:00:00\t{who}\t{kind}\t{}\t{}\t{}",
            at[0], at[1], at[2]
        )
    }

    #[test]
    fn a_lookup_answers_the_newest_changes_within_the_radius_first() {
        let lines = [
            line("Ann (01)", PLACED, [0, 0, 0]),
            line("Bob (02)", DUG, [3, 4, 0]),
            line("Cid (03)", TORN_DOWN, [50, 0, 0]),
            "not a line".to_owned(),
        ];
        let found = near(&lines, [0.0, 0.0, 0.0], 5.0);
        let who: Vec<&str> = found.iter().map(|change| change.who.as_str()).collect();
        assert_eq!(who, ["Bob (02)", "Ann (01)"], "5 m away is within 5 m");
        assert_eq!(found[0].kind_text(), texts::BUILD_DUG);
        assert_eq!(near(&lines, [0.0, 0.0, 0.0], 4.9).len(), 1);
        let many: Vec<String> = (0..MAX_FOUND + 5).map(|_| lines[0].clone()).collect();
        assert_eq!(near(&many, [0.0, 0.0, 0.0], 1.0).len(), MAX_FOUND);
    }

    #[test]
    fn a_line_with_a_place_that_is_no_number_is_left_out() {
        assert_eq!(Change::parse("t\tAnn\tdug\tNaN\t0\t0"), None);
        assert_eq!(Change::parse("t\tAnn\tdug\t1\t2"), None);
        let odd = Change::parse("t\tAnn\tmelted\t1\t2\t3").unwrap();
        assert_eq!(odd.kind_text(), "melted", "an unknown word reads as it is");
    }

    #[test]
    fn digging_on_in_one_spot_is_one_line_until_the_kind_the_cell_or_the_time_moves_on() {
        let (ann, start) = (PlayerKey::new(1), Instant::now());
        let mut spots = Spots::default();
        assert!(spots.is_new(ann, DUG, [1.0, 1.0, 1.0], start));
        assert!(!spots.is_new(ann, DUG, [7.9, 1.0, 1.0], start));
        assert!(spots.is_new(PlayerKey::new(2), DUG, [1.0, 1.0, 1.0], start));
        assert!(spots.is_new(ann, FILLED, [1.0, 1.0, 1.0], start));
        assert!(spots.is_new(ann, FILLED, [8.0, 1.0, 1.0], start));
        assert!(!spots.is_new(ann, FILLED, [8.0, 1.0, 1.0], start + SAME_SPOT / 2));
        assert!(spots.is_new(ann, FILLED, [8.0, 1.0, 1.0], start + SAME_SPOT));
        assert_eq!(spots.0.len(), 2, "one spot a player");
        spots.clear();
        assert!(spots.is_new(ann, FILLED, [8.0, 1.0, 1.0], start));
    }

    proptest! {
        #[test]
        fn a_written_change_reads_back(x in -99_999i32..99_999, y in -999i32..9_999, z in -99_999i32..99_999, kind in 0usize..KINDS.len()) {
            let change = Change::parse(&line("Ann (0000000000000001)", KINDS[kind].0, [x, y, z])).unwrap();
            prop_assert_eq!(change.at.map(f64::to_bits), [x, y, z].map(|axis| f64::from(axis).to_bits()));
            prop_assert_eq!(change.kind_text(), KINDS[kind].1);
        }

        #[test]
        fn no_line_makes_the_reader_panic(line in ".*") {
            let read = Change::parse(&line);
            prop_assert!(read.is_none_or(|change| change.at.iter().all(|axis| axis.is_finite())));
        }
    }
}
