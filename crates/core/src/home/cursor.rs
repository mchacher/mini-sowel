//! Long lists written in chunks (memory ceiling, spec 010 R7.15): a cursor holds the ids of the
//! items in the order they are written, and each call serialises the next few from the home as
//! it is at that moment, so a whole list is never built. The same cursor serves the API's long
//! lists (`GET /devices`, `GET /equipments`) and the files written to storage. A client or a file
//! sees one JSON array; an item deleted between two chunks is skipped, one added is not listed.
//! `settings.json` is the one object among them: six short values, written whole in one chunk.

use std::io::{self, Write};

use serde::Serialize;

use super::Home;
use crate::devices::{Id, stored_item};
use crate::equipments::{EquipmentType, details};
use crate::recipes::StateKeys;

/// Items written per chunk.
pub const CHUNK_ITEMS: usize = 4;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Kind {
    /// The API's device list.
    Devices,
    /// The API's equipment list, with details.
    Equipments,
    /// `devices.json`.
    SavedDevices,
    /// `zones.json`.
    SavedZones,
    /// `equipments.json`.
    SavedEquipments,
    /// The API's recipe instances, each with its `state`.
    RecipeInstances,
    /// `recipes.json`.
    SavedRecipes,
    /// `settings.json`: one JSON object, written whole in one chunk (six values at most).
    SavedSettings,
}

/// Where a streamed list stands. At most 64 ids (devices, equipments and zones are bounded, and
/// so are the 16 recipe instances).
#[derive(Debug)]
pub struct ListCursor {
    kind: Kind,
    ids: Vec<Id>,
    next: usize,
    opened: bool,
    wrote_any: bool,
    done: bool,
    now: u64,
}

/// An instance with its state keys, as `GET /recipe-instances` lists it.
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct WithState<'a> {
    id: Id,
    recipe_id: &'a str,
    // Raw text, which `flatten` cannot carry: the instance's fields are written out in order.
    #[serde(with = "crate::recipes::raw_json")]
    params: &'a str,
    enabled: bool,
    #[serde(with = "crate::devices::iso")]
    created_at: u64,
    state: StateKeys,
}

/// Counts the bytes written to it and keeps none.
struct Counter(usize);

impl Write for Counter {
    fn write(&mut self, bytes: &[u8]) -> io::Result<usize> {
        self.0 += bytes.len();
        Ok(bytes.len())
    }

    fn flush(&mut self) -> io::Result<()> {
        Ok(())
    }
}

impl ListCursor {
    /// The API's devices by `createdAt` (stable).
    pub fn devices(home: &Home, now: u64) -> Self {
        let mut items: Vec<_> = home.devices.list().iter().collect();
        items.sort_by_key(|d| d.device.created_at);
        ListCursor::new(
            Kind::Devices,
            items.iter().map(|d| d.device.id).collect(),
            now,
        )
    }

    /// The API's equipments by name, of one type when `filter` is given (`None`: every one;
    /// `Some(None)`: a type that does not exist, so none).
    pub fn equipments(home: &Home, filter: Option<Option<EquipmentType>>, now: u64) -> Self {
        let mut items: Vec<_> = match filter {
            Some(None) => Vec::new(),
            Some(Some(kind)) => home
                .equipments()
                .list()
                .iter()
                .filter(|e| e.equipment.kind == kind)
                .collect(),
            None => home.equipments().list().iter().collect(),
        };
        // By exact name, byte by byte, as SQLite's `ORDER BY name` (uppercase before lowercase).
        items.sort_by(|a, b| a.equipment.name.cmp(&b.equipment.name));
        ListCursor::new(
            Kind::Equipments,
            items.iter().map(|e| e.equipment.id).collect(),
            now,
        )
    }

    /// `devices.json`: every device, in the order they are kept.
    pub fn saved_devices(home: &Home) -> Self {
        let ids = home.devices.list().iter().map(|d| d.device.id).collect();
        ListCursor::new(Kind::SavedDevices, ids, 0)
    }

    /// `zones.json`.
    pub fn saved_zones(home: &Home) -> Self {
        let ids = home.zones().list().iter().map(|z| z.id).collect();
        ListCursor::new(Kind::SavedZones, ids, 0)
    }

    /// `equipments.json`.
    pub fn saved_equipments(home: &Home) -> Self {
        let ids = home
            .equipments()
            .list()
            .iter()
            .map(|e| e.equipment.id)
            .collect();
        ListCursor::new(Kind::SavedEquipments, ids, 0)
    }

    /// The API's recipe instances, oldest first, each with its `state`.
    pub fn recipe_instances(home: &Home) -> Self {
        let ids = home.recipes().instances().map(|i| i.id).collect();
        ListCursor::new(Kind::RecipeInstances, ids, 0)
    }

    /// `recipes.json`: the instances only.
    pub fn saved_recipes(home: &Home) -> Self {
        let ids = home.recipes().instances().map(|i| i.id).collect();
        ListCursor::new(Kind::SavedRecipes, ids, 0)
    }

    /// `settings.json`: the settings as they are when the chunk is written.
    pub fn saved_settings() -> Self {
        ListCursor::new(Kind::SavedSettings, Vec::new(), 0)
    }

    fn new(kind: Kind, ids: Vec<Id>, now: u64) -> Self {
        ListCursor {
            kind,
            ids,
            next: 0,
            opened: false,
            wrote_any: false,
            done: false,
            now,
        }
    }

    /// Writes the next piece of the array into `w` and says where the cursor then stands:
    /// (next index, something written, finished). Does not change the cursor.
    pub(crate) fn render<W: Write>(
        &self,
        home: &Home,
        w: &mut W,
    ) -> io::Result<(usize, bool, bool)> {
        if self.kind == Kind::SavedSettings {
            w.write_all(home.settings().to_json().as_bytes())?;
            return Ok((0, true, true));
        }
        let mut wrote_any = self.wrote_any;
        if !self.opened {
            w.write_all(b"[")?;
        }
        let end = (self.next + CHUNK_ITEMS).min(self.ids.len());
        for id in &self.ids[self.next..end] {
            // The comma goes before an item that exists; a missing one writes nothing.
            if self.exists(home, id) {
                if wrote_any {
                    w.write_all(b",")?;
                }
                self.write_item(home, id, w)?;
                wrote_any = true;
            }
        }
        let finished = end >= self.ids.len();
        if finished {
            w.write_all(b"]")?;
        }
        Ok((end, wrote_any, finished))
    }

    fn exists(&self, home: &Home, id: &Id) -> bool {
        match self.kind {
            Kind::Devices | Kind::SavedDevices => home.devices.get_by_id(id).is_some(),
            Kind::SavedSettings => false,
            Kind::SavedZones => home.zones().get(id).is_some(),
            Kind::RecipeInstances | Kind::SavedRecipes => home.recipes().get(id).is_some(),
            Kind::Equipments | Kind::SavedEquipments => {
                home.equipments().get(&id.to_string()).is_some()
            }
        }
    }

    fn write_item<W: Write>(&self, home: &Home, id: &Id, w: &mut W) -> io::Result<()> {
        let result = match self.kind {
            Kind::Devices => match home.devices.get_by_id(id) {
                Some(item) => serde_json::to_writer(&mut *w, item),
                None => return Ok(()),
            },
            Kind::SavedDevices => match home.devices.get_by_id(id) {
                Some(item) => serde_json::to_writer(&mut *w, &stored_item(item)),
                None => return Ok(()),
            },
            Kind::SavedZones => match home.zones().get(id) {
                Some(zone) => serde_json::to_writer(&mut *w, zone),
                None => return Ok(()),
            },
            Kind::Equipments => match home.equipments().get(&id.to_string()) {
                Some(item) => {
                    serde_json::to_writer(&mut *w, &details(&home.devices, item, self.now))
                }
                None => return Ok(()),
            },
            Kind::SavedEquipments => match home.equipments().get(&id.to_string()) {
                Some(item) => item.serialize(&mut serde_json::Serializer::new(&mut *w)),
                None => return Ok(()),
            },
            Kind::SavedSettings => return Ok(()),
            Kind::SavedRecipes => match home.recipes().get(id) {
                Some(instance) => serde_json::to_writer(&mut *w, instance),
                None => return Ok(()),
            },
            Kind::RecipeInstances => match home.recipes().get(id) {
                Some(instance) => serde_json::to_writer(
                    &mut *w,
                    &WithState {
                        id: instance.id,
                        recipe_id: &instance.recipe_id,
                        params: &instance.params,
                        enabled: instance.enabled,
                        created_at: instance.created_at,
                        state: home.recipes().state_of(id).unwrap_or_default(),
                    },
                ),
                None => return Ok(()),
            },
        };
        result.map_err(io::Error::from)
    }

    /// The next piece of the JSON array (at most `CHUNK_ITEMS` items, with the `[`, the commas
    /// and the `]` where they belong), or `None` once the array is complete. The chunk is
    /// measured first and built in a buffer of its exact size, so it never doubles.
    ///
    /// A failure to serialise is an `Err`, never a `None`: the caller must stop (the HTTP answer
    /// is aborted, a file is never renamed), not take a truncated list for a complete one. The
    /// cursor does not advance on an error.
    pub fn next_chunk(&mut self, home: &Home) -> Result<Option<String>, io::Error> {
        if self.done {
            return Ok(None);
        }
        let mut counter = Counter(0);
        self.render(home, &mut counter)?;
        let mut bytes = Vec::with_capacity(counter.0);
        let (end, wrote_any, finished) = self.render(home, &mut bytes)?;
        let text =
            String::from_utf8(bytes).map_err(|e| io::Error::new(io::ErrorKind::InvalidData, e))?;
        self.opened = true;
        self.next = end;
        self.wrote_any = wrote_any;
        self.done = finished;
        Ok(Some(text))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A writer that fails after a number of bytes.
    struct Failing(usize);

    impl Write for Failing {
        fn write(&mut self, bytes: &[u8]) -> io::Result<usize> {
            if bytes.len() > self.0 {
                return Err(io::Error::other("injected failure"));
            }
            self.0 -= bytes.len();
            Ok(bytes.len())
        }

        fn flush(&mut self) -> io::Result<()> {
            Ok(())
        }
    }

    #[test]
    fn a_failure_while_writing_a_chunk_is_an_error_not_the_end_of_the_list() {
        let mut home = Home::new(1_795_252_323_120);
        let mut random = |buffer: &mut [u8]| {
            buffer.fill(7);
            Ok(())
        };
        home.create_zone(
            crate::zones::NewZone {
                name: "Z".into(),
                ..Default::default()
            },
            1,
            &mut random,
        )
        .unwrap();
        let cursor = ListCursor::saved_zones(&home);
        // The chunk renders when the writer holds out ...
        let mut bytes = Vec::new();
        assert!(cursor.render(&home, &mut bytes).is_ok());
        // ... and the error is the writer's, at any point of it.
        for limit in [0, 1, 10, bytes.len() - 1] {
            assert!(
                cursor.render(&home, &mut Failing(limit)).is_err(),
                "{limit}"
            );
        }
    }
}
