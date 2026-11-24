//! Zones (spec 010): Sowel's `Zone` and `ZoneWithChildren` (`src/shared/types.ts` T:268, T:279)
//! and the rules of `src/zones/zone-manager.ts`: a tree under a fixed root zone, a zone never its
//! own ancestor, never deleted while it holds zones or equipments. No I/O: the caller writes.

pub mod aggregate;

use serde::{Deserialize, Serialize, Serializer};

use crate::devices::Id;
use crate::devices::iso;
use crate::failure::{Failure, Outcome};

/// Zones kept, the root included; a zone past it is refused (400).
pub const MAX_ZONES: usize = 32;
/// The longest zone name, in characters (Sowel allows 100).
pub const MAX_NAME_CHARS: usize = 64;
/// The longest icon name.
pub const MAX_ICON_CHARS: usize = 32;
/// The longest description.
pub const MAX_DESCRIPTION_CHARS: usize = 500;

/// Sowel's `Zone`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Zone {
    pub id: Id,
    pub name: String,
    /// `null` for the root zone only.
    pub parent_id: Option<Id>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub icon: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    pub display_order: i64,
    #[serde(with = "iso")]
    pub created_at: u64,
    #[serde(with = "iso")]
    pub updated_at: u64,
}

/// What `POST /zones` gives. `parent` is the id as written (`None`: the root zone).
#[derive(Debug, Default)]
pub struct NewZone {
    pub name: String,
    pub parent: Option<String>,
    pub icon: Option<String>,
    pub description: Option<String>,
    pub display_order: i64,
}

/// What `PUT /zones/:id` gives: each field absent (keep), or its new value; for `parent`, `icon`
/// and `description` the inner `None` is JSON `null`.
#[derive(Debug, Default)]
pub struct ZoneUpdate {
    pub name: Option<String>,
    pub parent: Option<Option<String>>,
    pub icon: Option<Option<String>>,
    pub description: Option<Option<String>>,
    pub display_order: Option<i64>,
}

/// The zones, at most `MAX_ZONES`, the root always among them. A clone is the copy an import of
/// a backup works on (spec 026 R3.2).
#[derive(Debug, Clone)]
pub struct Zones {
    items: Vec<Zone>,
}

fn root(now: u64) -> Zone {
    Zone {
        id: Id::ROOT_ZONE,
        name: "Maison".into(),
        parent_id: None,
        icon: Some("home".into()),
        description: None,
        display_order: 0,
        created_at: now,
        updated_at: now,
    }
}

/// Zones in Sowel's order: `display_order`, then `name` (SQLite's binary comparison).
fn by_order(a: &&Zone, b: &&Zone) -> std::cmp::Ordering {
    a.display_order
        .cmp(&b.display_order)
        .then_with(|| a.name.cmp(&b.name))
}

impl Zones {
    /// Only the root zone (first start).
    pub fn new(now: u64) -> Self {
        Zones {
            items: vec![root(now)],
        }
    }

    /// The zones read from storage, repaired as Sowel's `ensureRootZone` does: the root zone is
    /// created when missing, and a zone whose parent is missing, or whose parents never lead to
    /// the root, goes under the root. At most `MAX_ZONES` are kept. True when something was
    /// repaired or created (the caller then writes the file).
    pub fn from_list(mut list: Vec<Zone>, now: u64) -> (Self, bool) {
        let mut repaired = false;
        let before = list.len();
        // A second zone with the same id is dropped.
        let mut seen: Vec<Id> = Vec::new();
        list.retain(|z| {
            if seen.contains(&z.id) {
                false
            } else {
                seen.push(z.id);
                true
            }
        });
        repaired |= list.len() != before;
        if !list.iter().any(|z| z.id == Id::ROOT_ZONE) {
            list.insert(0, root(now));
            repaired = true;
        }
        // The root is the one zone without a parent, whatever the file says.
        if let Some(r) = list.iter_mut().find(|z| z.id == Id::ROOT_ZONE)
            && r.parent_id.is_some()
        {
            r.parent_id = None;
            repaired = true;
        }
        if list.len() > MAX_ZONES {
            list.truncate(MAX_ZONES);
            // The root must be kept.
            if !list.iter().any(|z| z.id == Id::ROOT_ZONE) {
                list.pop();
                list.insert(0, root(now));
            }
            repaired = true;
        }
        let mut zones = Zones { items: list };
        for index in 0..zones.items.len() {
            let zone = &zones.items[index];
            if zone.id != Id::ROOT_ZONE && !zones.reaches_root(&zone.id) {
                zones.items[index].parent_id = Some(Id::ROOT_ZONE);
                repaired = true;
            }
        }
        (zones, repaired)
    }

    pub fn list(&self) -> &[Zone] {
        &self.items
    }

    pub fn get(&self, id: &Id) -> Option<&Zone> {
        self.items.iter().find(|z| z.id == *id)
    }

    /// The zone with this id as written (`None` for anything that is not an id of a zone).
    pub fn find(&self, id: &str) -> Option<&Zone> {
        Id::parse(id).and_then(|id| self.get(&id))
    }

    /// True when walking up from the zone reaches the root zone (never loops: bounded).
    fn reaches_root(&self, id: &Id) -> bool {
        let mut current = *id;
        for _ in 0..=MAX_ZONES {
            if current == Id::ROOT_ZONE {
                return true;
            }
            match self.get(&current).and_then(|z| z.parent_id) {
                Some(parent) => current = parent,
                None => return false,
            }
        }
        false
    }

    /// True when `id` is `ancestor` or lies below it.
    pub fn is_within(&self, id: &Id, ancestor: &Id) -> bool {
        let mut current = *id;
        for _ in 0..=MAX_ZONES {
            if current == *ancestor {
                return true;
            }
            match self.get(&current).and_then(|z| z.parent_id) {
                Some(parent) => current = parent,
                None => return false,
            }
        }
        false
    }

    /// The direct children of a zone, in order.
    fn children_of<'a>(&'a self, id: &Id) -> Vec<&'a Zone> {
        let mut children: Vec<&Zone> = self
            .items
            .iter()
            .filter(|z| z.parent_id == Some(*id))
            .collect();
        children.sort_by(by_order);
        children
    }

    /// `GET /zones`: the root with all its descendants.
    pub fn tree(&self) -> Option<Tree<'_>> {
        self.get(&Id::ROOT_ZONE).map(|zone| Tree {
            zones: self,
            zone,
            levels: u8::MAX,
        })
    }

    /// `GET /zones/:id`: the zone, its children, and their children with `children: []`
    /// (`getByIdWithChildren`, `zone-manager.ts:176`).
    pub fn with_children(&self, id: &str) -> Option<Tree<'_>> {
        self.find(id).map(|zone| Tree {
            zones: self,
            zone,
            levels: 2,
        })
    }

    pub fn create(&mut self, input: NewZone, now: u64, id: Id) -> Outcome<Zone> {
        let parent = match &input.parent {
            None => Id::ROOT_ZONE,
            Some(text) => self
                .find(text)
                .map(|z| z.id)
                .ok_or_else(|| Failure::new(404, format!("Parent zone not found: {text}")))?,
        };
        if self.items.len() >= MAX_ZONES {
            return Err(Failure::new(400, format!("Too many zones ({MAX_ZONES})")));
        }
        let zone = Zone {
            id,
            name: input.name,
            parent_id: Some(parent),
            icon: input.icon,
            description: input.description,
            display_order: input.display_order,
            created_at: now,
            updated_at: now,
        };
        self.items.push(zone.clone());
        Ok(zone)
    }

    pub fn update(&mut self, id: &str, input: ZoneUpdate, now: u64) -> Outcome<Zone> {
        let Some(existing) = self.find(id) else {
            return Err(Failure::new(404, "Zone not found"));
        };
        let id = existing.id;
        let parent = match input.parent {
            None => existing.parent_id,
            Some(None) if id != Id::ROOT_ZONE => {
                return Err(Failure::new(400, "Only the root zone has no parent"));
            }
            Some(None) => None,
            Some(Some(text)) => {
                // The parent must exist; then the own-parent and the circular tests.
                let Some(parent) = self.find(&text).map(|z| z.id) else {
                    return Err(Failure::new(404, format!("Parent zone not found: {text}")));
                };
                if parent == id {
                    return Err(Failure::new(400, "A zone cannot be its own parent"));
                }
                if self.is_within(&parent, &id) {
                    return Err(Failure::new(
                        400,
                        "Moving this zone would create a circular reference",
                    ));
                }
                Some(parent)
            }
        };
        let Some(zone) = self.items.iter_mut().find(|z| z.id == id) else {
            return Err(Failure::new(404, "Zone not found"));
        };
        if let Some(name) = input.name {
            zone.name = name;
        }
        zone.parent_id = parent;
        if let Some(icon) = input.icon {
            zone.icon = icon;
        }
        if let Some(description) = input.description {
            zone.description = description;
        }
        if let Some(order) = input.display_order {
            zone.display_order = order;
        }
        zone.updated_at = now;
        Ok(zone.clone())
    }

    /// Removes a zone that has no zone and no equipment below it (`equipments`: how many the
    /// caller counted in it). The rules and texts are Sowel's `ZoneManager.delete`.
    pub fn delete(&mut self, id: &str, equipments: impl Fn(&Id) -> usize) -> Outcome<()> {
        let parsed = Id::parse(id);
        if parsed == Some(Id::ROOT_ZONE) {
            return Err(Failure::new(400, "Cannot delete the root zone"));
        }
        let Some(zone) = self.find(id) else {
            return Err(Failure::new(404, "Zone not found"));
        };
        let zone_id = zone.id;
        let children = self
            .items
            .iter()
            .filter(|z| z.parent_id == Some(zone_id))
            .count();
        if children > 0 {
            let s = if children > 1 { "s" } else { "" };
            return Err(Failure::new(
                400,
                format!(
                    "Cannot delete zone with {children} child zone{s}. Remove or move them first."
                ),
            ));
        }
        let count = equipments(&zone_id);
        if count > 0 {
            let s = if count > 1 { "s" } else { "" };
            return Err(Failure::new(
                400,
                format!("Cannot delete zone with {count} equipment{s}. Remove or move them first."),
            ));
        }
        self.items.retain(|z| z.id != zone_id);
        Ok(())
    }

    /// The zones as the JSON list that is saved.
    pub fn to_json(&self) -> Result<String, String> {
        crate::devices::to_json(&self.items).ok_or_else(|| "cannot serialise the zones".into())
    }
}

/// The zones read back from the JSON `Zones::to_json` wrote.
pub fn parse_list(json: &str) -> Result<Vec<Zone>, String> {
    serde_json::from_str(json).map_err(|e| e.to_string())
}

/// Sowel's `ZoneWithChildren`, serialised straight from the zones: no copy of the tree.
pub struct Tree<'a> {
    zones: &'a Zones,
    zone: &'a Zone,
    /// How many levels of children are written below this zone.
    levels: u8,
}

#[derive(Serialize)]
struct Node<'a> {
    #[serde(flatten)]
    zone: &'a Zone,
    children: Children<'a>,
}

struct Children<'a> {
    zones: &'a Zones,
    id: Id,
    levels: u8,
}

impl Serialize for Children<'_> {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        if self.levels == 0 {
            return serializer.collect_seq(std::iter::empty::<u8>());
        }
        serializer.collect_seq(
            self.zones
                .children_of(&self.id)
                .into_iter()
                .map(|zone| Tree {
                    zones: self.zones,
                    zone,
                    levels: self.levels - 1,
                }),
        )
    }
}

impl Serialize for Tree<'_> {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        Node {
            zone: self.zone,
            children: Children {
                zones: self.zones,
                id: self.zone.id,
                levels: self.levels,
            },
        }
        .serialize(serializer)
    }
}
