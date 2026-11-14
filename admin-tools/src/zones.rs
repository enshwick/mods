//! Protected zones: circles and boxes where only the players the world's
//! rule names may build, kept in the world's data. The server refuses every
//! other placement in an active zone after the game's own check, so a
//! client without the mod is refused too.

use std::sync::Arc;

use enshwick::actions::{Action, Arg, Outcome, Request, Value};
use enshwick::vetoes::BuildVerdict;
use enshwick::{Entity, Load, Off};
use serde::{Deserialize, Serialize};

use crate::players::{bad_args, character};
use crate::settings::Who;
use crate::{Tools, ids, texts};

/// The world store's file name, `zones.toml`.
pub(crate) const STORE: &str = "zones";

/// The most zones a world keeps, so the build filter's walk stays short.
pub(crate) const MAX_ZONES: usize = 64;

/// The longest zone name, in bytes.
pub(crate) const MAX_NAME: usize = 32;

/// The widest radius of a circle, in metres.
const MAX_RADIUS: f64 = 1_000.0;

/// The longest side of a box, in metres.
const MAX_SIDE: f64 = 2_000.0;

/// The shortest radius or side, in metres.
const MIN_SIZE: f64 = 1.0;

/// The zones of a world.
#[derive(Debug, Default, Clone, PartialEq, Serialize, Deserialize)]
pub(crate) struct Zones {
    /// The zones, in the order added.
    pub zones: Vec<Zone>,
}

/// A protected zone.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub(crate) struct Zone {
    /// What administrators call it, unique in any case.
    pub name: String,
    /// Where it lies.
    pub shape: Shape,
    /// Whether it refuses building now.
    pub on: bool,
    /// Who made it.
    pub by: String,
}

/// Where a zone lies, in metres.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "lowercase")]
pub(crate) enum Shape {
    /// A circle on the map, at every height: its centre's x and z and its
    /// radius.
    Circle { x: f64, z: f64, radius: f64 },
    /// A box between two corners, `min` below `max` on each axis.
    Box { min: [f64; 3], max: [f64; 3] },
}

impl Shape {
    /// A box of `size` centred on `centre`.
    fn around(centre: [f64; 3], size: [f64; 3]) -> Self {
        Self::Box {
            min: core::array::from_fn(|axis| centre[axis] - size[axis] / 2.0),
            max: core::array::from_fn(|axis| centre[axis] + size[axis] / 2.0),
        }
    }

    /// Whether the actions could have made it: finite, within the sizes
    /// they take, so a hand-edited file cannot cover the whole map.
    fn usable(&self) -> bool {
        match *self {
            Self::Circle { x, z, radius } => {
                x.is_finite() && z.is_finite() && (MIN_SIZE..=MAX_RADIUS).contains(&radius)
            }
            Self::Box { min, max } => min.iter().zip(&max).all(|(low, high)| {
                low.is_finite() && (MIN_SIZE..=MAX_SIDE).contains(&(high - low))
            }),
        }
    }

    /// Whether `at` lies inside, edges included; a shape with a NaN holds
    /// nothing.
    pub(crate) fn holds(&self, [x, y, z]: [f64; 3]) -> bool {
        match *self {
            Self::Circle {
                x: cx,
                z: cz,
                radius,
            } => (x - cx).powi(2) + (z - cz).powi(2) <= radius * radius,
            Self::Box { min, max } => [x, y, z]
                .iter()
                .zip(min.iter().zip(&max))
                .all(|(at, (low, high))| (low..=high).contains(&at)),
        }
    }
}

/// Why a zone was not added.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum AddZoneError {
    /// A zone has the name already.
    Taken,
    /// The world has [`MAX_ZONES`].
    Full,
}

/// What the build filter guards while the world's zones do not read: every
/// place, as the platform asks of a value that guards something.
const EVERYWHERE: Shape = Shape::Box {
    min: [f64::NEG_INFINITY; 3],
    max: [f64::INFINITY; 3],
};

/// Whether `a` and `b` are one name in any case, in any script.
fn same_name(a: &str, b: &str) -> bool {
    a.chars()
        .flat_map(char::to_lowercase)
        .eq(b.chars().flat_map(char::to_lowercase))
}

impl Zones {
    /// Where the zone named `name` is, in any case.
    fn find(&self, name: &str) -> Option<usize> {
        self.zones
            .iter()
            .position(|zone| same_name(&zone.name, name))
    }

    /// Adds `zone`, switched on.
    pub(crate) fn add(&mut self, zone: Zone) -> Result<(), AddZoneError> {
        if self.find(&zone.name).is_some() {
            return Err(AddZoneError::Taken);
        }
        if self.zones.len() >= MAX_ZONES {
            return Err(AddZoneError::Full);
        }
        self.zones.push(zone);
        Ok(())
    }

    /// Removes the zone named `name`, and any a hand edit gave the same
    /// name; its name as kept.
    pub(crate) fn remove(&mut self, name: &str) -> Option<String> {
        let kept = self.zones[self.find(name)?].name.clone();
        self.zones.retain(|zone| !same_name(&zone.name, name));
        Some(kept)
    }

    /// Switches the zone named `name` on or off, and any a hand edit gave
    /// the same name; its name as kept.
    pub(crate) fn switch(&mut self, name: &str, on: bool) -> Option<String> {
        let kept = self.zones[self.find(name)?].name.clone();
        for zone in &mut self.zones {
            if same_name(&zone.name, name) {
                zone.on = on;
            }
        }
        Some(kept)
    }

    /// The shapes of the zones that are on and usable, at most
    /// [`MAX_ZONES`] of a file edited by hand.
    pub(crate) fn guarded(&self) -> Vec<Shape> {
        self.zones
            .iter()
            .filter(|zone| zone.on && zone.shape.usable())
            .take(MAX_ZONES)
            .map(|zone| zone.shape)
            .collect()
    }

    /// The names of the zones a hand edit left with a shape the actions
    /// would not make, which guard nothing.
    fn unusable(&self) -> Vec<&str> {
        self.zones
            .iter()
            .filter(|zone| !zone.shape.usable())
            .map(|zone| zone.name.as_str())
            .collect()
    }
}

/// Declares the zone actions and the build filter that keeps them; without
/// the filter on this build, neither.
pub(crate) fn add(tools: &Arc<Tools>, load: &mut Load<'_>) -> Result<(), Off> {
    let filtered = Arc::clone(tools);
    // Runs on every frame a player aims to build: no lock, allocation or log.
    let filter = load.on_build(move |placement| {
        let guarded = filtered.guarded.load();
        if !guarded.iter().any(|shape| shape.holds(placement.position))
            || filtered.zone_builders.load().contains(&placement.player)
        {
            return BuildVerdict::Allow;
        }
        BuildVerdict::refuse_because(texts::NOTICE_ZONE)
    });
    if let Err(why) = filter {
        enshwick::warn!("protected zones are off: {why}");
        return Ok(());
    }
    let name = || Arg::text(texts::ARG_ZONE, MAX_NAME);
    let size = |label| Arg::number(label, MIN_SIZE..=MAX_SIDE, 0.0);
    tools.add(
        Action::new(ids::ZONE, texts::ACTION_ZONE)
            .help(texts::ACTION_ZONE_HELP)
            .arg(name())
            .arg(Arg::number(texts::ARG_RADIUS, MIN_SIZE..=MAX_RADIUS, 0.0)),
        zone,
    )?;
    tools.add(
        Action::new(ids::ZONE_BOX, texts::ACTION_ZONEBOX)
            .help(texts::ACTION_ZONEBOX_HELP)
            .arg(name())
            .arg(size(texts::ARG_WIDTH))
            .arg(size(texts::ARG_HEIGHT))
            .arg(size(texts::ARG_DEPTH)),
        zone_box,
    )?;
    tools.add(
        Action::new(ids::ZONES, texts::ACTION_ZONES).help(texts::ACTION_ZONES_HELP),
        list,
    )?;
    tools.add(
        Action::new(ids::ZONE_SWITCH, texts::ACTION_ZONESWITCH)
            .help(texts::ACTION_ZONESWITCH_HELP)
            .arg(name())
            .arg(Arg::toggle(texts::ARG_ON)),
        switch,
    )?;
    tools.add(
        Action::new(ids::UNZONE, texts::ACTION_UNZONE)
            .help(texts::ACTION_UNZONE_HELP)
            .arg(name()),
        unzone,
    )
}

/// A metre value as replies write it.
fn metres(value: f64) -> String {
    format!("{value:.0}")
}

/// Protects a circle around the caller.
fn zone(tools: &Tools, request: &Request<'_>) -> Outcome {
    let [Value::Text(name), Value::Number(radius)] = request.args else {
        return bad_args(request);
    };
    let [x, _, z] = match here(tools, request) {
        Ok(at) => at,
        Err(refused) => return refused,
    };
    let shape = Shape::Circle {
        x,
        z,
        radius: *radius,
    };
    let reply = request.say(
        texts::REPLY_ZONE,
        &[
            (texts::value::NAME, name),
            (texts::value::RADIUS, &metres(*radius)),
            (texts::value::X, &metres(x)),
            (texts::value::Z, &metres(z)),
        ],
    );
    add_zone(tools, request, name, shape, reply)
}

/// Protects a box centred on the caller.
fn zone_box(tools: &Tools, request: &Request<'_>) -> Outcome {
    let [
        Value::Text(name),
        Value::Number(width),
        Value::Number(height),
        Value::Number(depth),
    ] = request.args
    else {
        return bad_args(request);
    };
    let at = match here(tools, request) {
        Ok(at) => at,
        Err(refused) => return refused,
    };
    let shape = Shape::around(at, [*width, *height, *depth]);
    let mut values = vec![(texts::value::NAME, name.clone())];
    values.extend(box_values(&shape));
    let reply = request.say(texts::REPLY_ZONEBOX, &crate::pairs(&values));
    add_zone(tools, request, name, shape, reply)
}

/// Where the caller stands, or the refusal for a caller without a
/// character.
fn here(tools: &Tools, request: &Request<'_>) -> Result<[f64; 3], Outcome> {
    let me = character(request)?;
    tools
        .world
        .position(me.entity)
        .ok_or_else(|| Outcome::refused(request.say(texts::REPLY_NO_CHARACTER, &[])))
}

/// Adds the zone `name` lying at `shape`, replying `reply` once it holds.
fn add_zone(
    tools: &Tools,
    request: &Request<'_>,
    name: &str,
    shape: Shape,
    reply: String,
) -> Outcome {
    let zone = Zone {
        name: name.to_owned(),
        shape,
        on: true,
        by: request.player.name.clone(),
    };
    match tools.zones.update(|zones| zones.add(zone)) {
        None => shut(tools, request),
        Some(Err(AddZoneError::Taken)) => {
            Outcome::refused(request.say(texts::REPLY_ZONE_TAKEN, &[(texts::value::NAME, name)]))
        }
        Some(Err(AddZoneError::Full)) => Outcome::refused(request.say(
            texts::REPLY_ZONES_FULL,
            &[(texts::value::MAX, &MAX_ZONES.to_string())],
        )),
        Some(Ok(())) => {
            tools.zones_changed();
            Outcome::done(reply)
        }
    }
}

/// A zone as a list row reads it: the text key for its shape and state,
/// and its name and where it lies.
pub(crate) fn row(zone: &Zone) -> (&'static str, Vec<(&'static str, String)>) {
    let mut values = vec![(texts::value::NAME, zone.name.clone())];
    // The state is in the key: a window's values reach the client untranslated.
    let key = match (zone.shape, zone.on) {
        (Shape::Circle { x, z, radius }, on) => {
            values.extend([
                (texts::value::RADIUS, metres(radius)),
                (texts::value::X, metres(x)),
                (texts::value::Z, metres(z)),
            ]);
            if on {
                texts::REPLY_ZONES_CIRCLE_ON
            } else {
                texts::REPLY_ZONES_CIRCLE_OFF
            }
        }
        (Shape::Box { .. }, on) => {
            values.extend(box_values(&zone.shape));
            if on {
                texts::REPLY_ZONES_BOX_ON
            } else {
                texts::REPLY_ZONES_BOX_OFF
            }
        }
    };
    (key, values)
}

/// A box's centre and size as replies write them.
fn box_values(shape: &Shape) -> Vec<(&'static str, String)> {
    let Shape::Box { min, max } = *shape else {
        return Vec::new();
    };
    let centre: [f64; 3] = core::array::from_fn(|axis| f64::midpoint(min[axis], max[axis]));
    let size: [f64; 3] = core::array::from_fn(|axis| max[axis] - min[axis]);
    vec![
        (texts::value::WIDTH, metres(size[0])),
        (texts::value::HEIGHT, metres(size[1])),
        (texts::value::DEPTH, metres(size[2])),
        (texts::value::X, metres(centre[0])),
        (texts::value::Y, metres(centre[1])),
        (texts::value::Z, metres(centre[2])),
    ]
}

/// Every zone, with its state and where it lies.
fn list(tools: &Tools, request: &Request<'_>) -> Outcome {
    let Some(zones) = tools.zones.with(|zones| zones.zones.clone()) else {
        return shut(tools, request);
    };
    if zones.is_empty() {
        return Outcome::done(request.say(texts::REPLY_ZONES_NONE, &[]));
    }
    let count = zones.len().to_string();
    let head = request.say(texts::REPLY_ZONES, &[(texts::value::COUNT, &count)]);
    let rows: Vec<String> = zones
        .iter()
        .take(MAX_ZONES)
        .map(|zone| {
            let (key, values) = row(zone);
            request.say(key, &crate::pairs(&values))
        })
        .collect();
    Outcome::done(crate::lines(request, &[head, rows.join(crate::LINE)]))
}

/// Switches a zone on or off.
fn switch(tools: &Tools, request: &Request<'_>) -> Outcome {
    let [Value::Text(name), Value::Toggle(on)] = request.args else {
        return bad_args(request);
    };
    let Some(found) = tools.zones.update(|zones| zones.switch(name, *on)) else {
        return shut(tools, request);
    };
    let Some(kept) = found else {
        return no_zone(request, name);
    };
    tools.zones_changed();
    let reply = if *on {
        texts::REPLY_ZONE_ON
    } else {
        texts::REPLY_ZONE_OFF
    };
    Outcome::done(request.say(reply, &[(texts::value::NAME, &kept)]))
}

/// Removes a zone.
fn unzone(tools: &Tools, request: &Request<'_>) -> Outcome {
    let [Value::Text(name)] = request.args else {
        return bad_args(request);
    };
    let Some(found) = tools.zones.update(|zones| zones.remove(name)) else {
        return shut(tools, request);
    };
    let Some(kept) = found else {
        return no_zone(request, name);
    };
    tools.zones_changed();
    Outcome::done(request.say(texts::REPLY_UNZONED, &[(texts::value::NAME, &kept)]))
}

/// The refusal of a name no zone has.
fn no_zone(request: &Request<'_>, name: &str) -> Outcome {
    Outcome::refused(request.say(texts::REPLY_NO_ZONE, &[(texts::value::NAME, name)]))
}

/// The refusal while the world's zones do not read.
fn shut(tools: &Tools, request: &Request<'_>) -> Outcome {
    crate::shut(request, tools.zones.shut_reason(), texts::REPLY_ZONES_SHUT)
}

/// What the build filter guards: nothing when the rule lets everyone
/// build, every place while the zones do not read, else the shapes of the
/// zones that are on.
fn guarding(everyone: bool, read: Option<Vec<Shape>>, shut: bool) -> Vec<Shape> {
    match read {
        _ if everyone => Vec::new(),
        Some(shapes) => shapes,
        None if shut => vec![EVERYWHERE],
        None => Vec::new(),
    }
}

impl Tools {
    /// Hands the build filter the zones that are on now and writes them,
    /// so a change holds even when the server stops before the world saves.
    pub(crate) fn zones_changed(&self) {
        self.guard_zones();
        Self::keep(&self.zones, STORE);
    }

    /// Hands the build filter what it guards in the running world, as
    /// [`guarding`] says, and who may build there; nothing on a build
    /// without the filter.
    pub(crate) fn guard_zones(&self) {
        if !self.declared(ids::ZONES) {
            return;
        }
        let read = self.zones.with(|zones| {
            let unusable = zones.unusable();
            if !unusable.is_empty() {
                enshwick::warn!("zones that guard nothing: {}", unusable.join(", "));
            }
            zones.guarded()
        });
        let shut = self.zones.shut_reason();
        if let Some(why) = &shut {
            enshwick::warn!("zones do not read, so only zone builders build: {why}");
        }
        let everyone = self.settings.load().zone_rule() == Who::Everyone;
        self.guarded
            .store(Arc::new(guarding(everyone, read, shut.is_some())));
        self.find_zone_builders();
    }

    /// Hands the build filter the characters of the players online whose
    /// user group the world's rule lets build in a zone.
    // shortcut: a respawned character is refused until the next tick (1 s), since the API raises no respawn event; rebuild on one when it does.
    pub(crate) fn find_zone_builders(&self) {
        let builders: Vec<Entity> = if self.guarded.load().is_empty() {
            Vec::new()
        } else {
            let rule = self.settings.load().zone_rule();
            self.world
                .players()
                .iter()
                .filter(|player| rule.admits(player))
                .map(|player| player.entity)
                .collect()
        };
        if **self.zone_builders.load() != builders {
            self.zone_builders.store(Arc::new(builders));
        }
    }

    /// Lets the build filter go when the world stops.
    pub(crate) fn drop_zones(&self) {
        self.guarded.store(Arc::new(Vec::new()));
        self.zone_builders.store(Arc::new(Vec::new()));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn zone(name: &str, shape: Shape) -> Zone {
        Zone {
            name: name.into(),
            shape,
            on: true,
            by: "Ann".into(),
        }
    }

    const MARKET: Shape = Shape::Circle {
        x: 10.0,
        z: -20.0,
        radius: 5.0,
    };

    #[test]
    fn a_circle_holds_every_height_within_its_radius_and_its_edge() {
        assert!(MARKET.holds([10.0, -500.0, -20.0]));
        assert!(MARKET.holds([10.0, 900.0, -20.0]));
        assert!(MARKET.holds([13.0, 0.0, -24.0]), "on the edge");
        assert!(!MARKET.holds([13.1, 0.0, -24.0]));
        assert!(!MARKET.holds([f64::NAN, 0.0, -20.0]));
    }

    #[test]
    fn a_box_holds_what_lies_between_its_corners_on_every_axis() {
        let hall = Shape::around([0.0, 10.0, 0.0], [10.0, 4.0, 6.0]);
        assert_eq!(
            hall,
            Shape::Box {
                min: [-5.0, 8.0, -3.0],
                max: [5.0, 12.0, 3.0]
            }
        );
        assert!(hall.holds([5.0, 8.0, -3.0]), "a corner");
        assert!(!hall.holds([0.0, 12.5, 0.0]), "above it");
        assert!(!hall.holds([0.0, 10.0, 3.5]));
    }

    #[test]
    fn a_name_is_taken_in_any_case_and_the_world_keeps_at_most_max_zones() {
        let mut zones = Zones::default();
        assert_eq!(zones.add(zone("Market", MARKET)), Ok(()));
        assert_eq!(zones.add(zone("market", MARKET)), Err(AddZoneError::Taken));
        for n in 1..MAX_ZONES {
            assert_eq!(zones.add(zone(&format!("z{n}"), MARKET)), Ok(()));
        }
        assert_eq!(zones.add(zone("one more", MARKET)), Err(AddZoneError::Full));
    }

    #[test]
    fn only_zones_that_are_on_guard_and_a_switch_or_removal_finds_any_case() {
        let mut zones = Zones::default();
        let hall = Shape::around([0.0; 3], [2.0; 3]);
        assert!(hall.usable() && MARKET.usable());
        zones.add(zone("Market", MARKET)).unwrap();
        zones.add(zone("Hall", hall)).unwrap();
        assert_eq!(zones.switch("MARKET", false).as_deref(), Some("Market"));
        assert_eq!(zones.guarded(), [hall]);
        assert_eq!(zones.switch("nowhere", true), None);
        assert_eq!(zones.remove("hall").as_deref(), Some("Hall"));
        assert!(zones.guarded().is_empty());
        assert_eq!(zones.remove("hall"), None);
    }

    #[test]
    fn a_name_is_one_in_any_case_in_any_script() {
        let mut zones = Zones::default();
        zones.add(zone("Ринок", MARKET)).unwrap();
        assert_eq!(zones.add(zone("ринок", MARKET)), Err(AddZoneError::Taken));
        assert_eq!(zones.remove("РИНОК").as_deref(), Some("Ринок"));
    }

    #[test]
    fn a_hand_edited_shape_the_actions_would_not_make_guards_nothing() {
        let mut zones = Zones::default();
        let huge = Shape::Circle {
            x: 0.0,
            z: 0.0,
            radius: f64::INFINITY,
        };
        let inside_out = Shape::Box {
            min: [5.0; 3],
            max: [-5.0; 3],
        };
        zones.add(zone("Market", MARKET)).unwrap();
        zones.add(zone("Map", huge)).unwrap();
        zones.add(zone("Odd", inside_out)).unwrap();
        assert_eq!(zones.guarded(), [MARKET]);
        assert_eq!(zones.unusable(), ["Map", "Odd"]);
    }

    #[test]
    fn a_switch_or_removal_reaches_every_zone_a_hand_edit_named_alike() {
        let mut zones = Zones {
            zones: vec![zone("Market", MARKET), zone("market", MARKET)],
        };
        assert_eq!(zones.switch("MARKET", false).as_deref(), Some("Market"));
        assert!(zones.guarded().is_empty());
        assert_eq!(zones.remove("market").as_deref(), Some("Market"));
        assert!(zones.zones.is_empty());
    }

    #[test]
    fn zones_that_do_not_read_guard_every_place_unless_everyone_builds() {
        assert_eq!(guarding(false, None, true), [EVERYWHERE]);
        assert!(EVERYWHERE.holds([1e12, -1e12, 0.0]));
        assert!(guarding(true, None, true).is_empty());
        assert!(guarding(true, Some(vec![MARKET]), false).is_empty());
        assert_eq!(guarding(false, Some(vec![MARKET]), false), [MARKET]);
        assert!(guarding(false, None, false).is_empty(), "no world runs");
    }

    #[test]
    fn zones_round_trip_through_their_file() {
        let mut zones = Zones::default();
        zones.add(zone("Market", MARKET)).unwrap();
        zones
            .add(zone("Hall", Shape::around([1.0; 3], [2.0; 3])))
            .unwrap();
        let text = toml::to_string(&zones).unwrap();
        assert!(text.contains("kind = \"circle\""), "{text}");
        assert_eq!(toml::from_str::<Zones>(&text).unwrap(), zones);
    }
}
