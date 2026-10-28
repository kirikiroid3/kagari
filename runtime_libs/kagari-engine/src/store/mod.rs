//! The two persistent stores: `settings.kisave` (the `Setting` statics) and `meta.kisave` (the
//! `Meta` statics, read flags, seen sets and the next autosave slot).

pub(crate) mod value;

use std::collections::{BTreeSet, HashMap, HashSet};

use kagari_archive::save::{Save, Scalar, ValueRef};
use kagari_bytecode::StaticKind;
use kagari_vm::{AssetId, Program, StaticId, Value};

use crate::Platform;
use value::{Names, decode, encode_field, same_shape};

const SETTINGS: &str = "settings.kisave";
const META: &str = "meta.kisave";

/// One of the seen sets: the assets of this program, plus the paths of assets it lacks.
#[derive(Default)]
pub(crate) struct SeenSet {
    pub ids: HashSet<AssetId>,
    retained: BTreeSet<String>,
}

pub(crate) struct Stores {
    pub settings: HashMap<StaticId, Value>,
    pub meta: HashMap<StaticId, Value>,
    pub read: HashSet<String>,
    pub seen_image: SeenSet,
    pub heard: SeenSet,
    pub watched: SeenSet,
    pub auto_next: u32,
    pub dirty_settings: bool,
    pub dirty_meta: bool,
}

impl Stores {
    /// Every `Setting` and `Meta` static at its initial value.
    pub fn initial(program: &Program) -> Stores {
        let mut stores = Stores {
            settings: HashMap::new(),
            meta: HashMap::new(),
            read: HashSet::new(),
            seen_image: SeenSet::default(),
            heard: SeenSet::default(),
            watched: SeenSet::default(),
            auto_next: 0,
            dirty_settings: false,
            dirty_meta: false,
        };
        for (index, s) in program.module().statics.iter().enumerate() {
            let Ok(index) = u32::try_from(index) else {
                break;
            };
            let Some(init) = program.constant(s.init).cloned() else {
                continue;
            };
            match s.kind {
                StaticKind::Setting => {
                    stores.settings.insert(StaticId(index), init);
                }
                StaticKind::Meta => {
                    stores.meta.insert(StaticId(index), init);
                }
                StaticKind::StoryVariable => {}
            }
        }
        stores
    }

    /// Records a line as read; whether it was new.
    pub fn set_read(&mut self, key: String) {
        if self.read.insert(key) {
            self.dirty_meta = true;
        }
    }

    /// Overlays the stored settings and meta.
    pub fn load(&mut self, program: &Program, platform: &mut dyn Platform) {
        if let Some(save) = read(platform, SETTINGS) {
            let root = save.root();
            overlay(program, SETTINGS, &mut self.settings, |path| root.get(path));
        }
        let Some(save) = read(platform, META) else {
            return;
        };
        let root = save.root();
        if let Some(ValueRef::Table(values)) = root.get("values") {
            overlay(program, META, &mut self.meta, |path| values.get(path));
        }
        if let Some(ValueRef::Table(read)) = root.get("read") {
            self.read = read.iter().map(|(k, _)| k.to_owned()).collect();
        }
        for (name, set) in [
            ("seen_image", &mut self.seen_image),
            ("heard", &mut self.heard),
            ("watched", &mut self.watched),
        ] {
            if let Some(ValueRef::Array(a)) = root.get(name) {
                for v in a.iter() {
                    if let ValueRef::String(path) = v {
                        match find_asset(program, path) {
                            Some(id) => set.ids.insert(id),
                            None => set.retained.insert(path.to_owned()),
                        };
                    }
                }
            }
        }
        match root.get("auto_next") {
            Some(ValueRef::U64(n)) => self.auto_next = u32::try_from(n).unwrap_or(0),
            Some(ValueRef::I64(n)) => self.auto_next = u32::try_from(n).unwrap_or(0),
            _ => {}
        }
    }

    /// Writes the dirty stores.
    pub fn flush(&mut self, names: &Names, platform: &mut dyn Platform) {
        if self.dirty_settings {
            self.dirty_settings = false;
            let mut save = Save::new();
            let ok = write_values(names, &mut save.root_mut(), &self.settings);
            write(platform, SETTINGS, ok.map(|()| save));
        }
        if self.dirty_meta {
            self.dirty_meta = false;
            let result = self.meta_save(names);
            write(platform, META, result);
        }
    }

    fn meta_save(&self, names: &Names) -> Result<Save, String> {
        let program = names.program;
        let mut save = Save::new();
        let mut root = save.root_mut();
        write_values(names, &mut root.table("values").map_err(err)?, &self.meta)?;
        {
            let mut read = root.table("read").map_err(err)?;
            let mut keys: Vec<&String> = self.read.iter().collect();
            keys.sort();
            for key in keys {
                read.set(key, Scalar::Bool(true)).map_err(err)?;
            }
        }
        for (name, set) in [
            ("seen_image", &self.seen_image),
            ("heard", &self.heard),
            ("watched", &self.watched),
        ] {
            let mut paths: BTreeSet<&str> = set.retained.iter().map(String::as_str).collect();
            for id in &set.ids {
                if let Some(a) = program.module().assets.get(id.0 as usize) {
                    paths.insert(program.string(a.path));
                }
            }
            let mut a = root.array(name).map_err(err)?;
            for path in paths {
                a.push(Scalar::String(path)).map_err(err)?;
            }
        }
        root.set("auto_next", Scalar::U64(u64::from(self.auto_next)))
            .map_err(err)?;
        Ok(save)
    }
}

fn err(e: impl std::fmt::Display) -> String {
    e.to_string()
}

fn write_values(
    names: &Names,
    table: &mut kagari_archive::save::TableMut<'_>,
    values: &HashMap<StaticId, Value>,
) -> Result<(), String> {
    let program = names.program;
    let mut sorted: Vec<(&str, &Value)> = values
        .iter()
        .filter_map(|(id, v)| {
            let s = program.module().statics.get(id.0 as usize)?;
            Some((program.string(s.path), v))
        })
        .collect();
    sorted.sort_by_key(|(path, _)| *path);
    for (path, v) in sorted {
        encode_field(names, table, path, v).map_err(|e| e.to_string())?;
    }
    Ok(())
}

fn write(platform: &mut dyn Platform, name: &str, save: Result<Save, String>) {
    let result = match save {
        Ok(save) => platform
            .write_store(name, save.as_bytes())
            .map_err(|e| e.to_string()),
        Err(e) => Err(e),
    };
    if let Err(e) = result {
        log::error!("{name}: cannot write: {e}");
    }
}

fn read(platform: &mut dyn Platform, name: &str) -> Option<Save> {
    match platform.read_store(name) {
        Ok(None) => None,
        Ok(Some(bytes)) => match Save::from_bytes(bytes) {
            Ok(save) => Some(save),
            Err(e) => {
                log::warn!("{name}: unreadable, using initial values: {e}");
                None
            }
        },
        Err(e) => {
            log::warn!("{name}: unreadable, using initial values: {e}");
            None
        }
    }
}

fn overlay<'a>(
    program: &Program,
    name: &str,
    values: &mut HashMap<StaticId, Value>,
    get: impl Fn(&str) -> Option<ValueRef<'a>>,
) {
    for (id, init) in values.iter_mut() {
        let Some(s) = program.module().statics.get(id.0 as usize) else {
            continue;
        };
        let path = program.string(s.path);
        let Some(stored) = get(path) else {
            continue;
        };
        match decode(program, stored) {
            Ok(v) if same_shape(&v, init) => *init = v,
            Ok(_) => log::warn!("{name}: `{path}` ignored: the stored value no longer fits"),
            Err(e) => log::warn!("{name}: `{path}` ignored: {e}"),
        }
    }
}

/// The asset at `path`.
pub(crate) fn find_asset(program: &Program, path: &str) -> Option<AssetId> {
    let index = program
        .module()
        .assets
        .iter()
        .position(|a| program.string(a.path) == path)?;
    Some(AssetId(u32::try_from(index).ok()?))
}
