//! The game system: the program description `main` returns, the settings the engine reads, and
//! the system state the imports, the UI and the screen draw consult.

pub(crate) mod crash;
pub(crate) mod imports;
pub(crate) mod modes;
pub(crate) mod nav;

use std::collections::HashSet;

use kagari_vm::{AssetId, Program, StaticId, Text, Value, Vm};

use crate::host::Bindings;
use crate::input::HeldInput;
use crate::input::bindings::{Binding, decode_bindings};
use crate::saves::SaveIndex;
use crate::store::Stores;
use crate::values::{StdTy, StdTypes, decode_handle, decode_int, decode_size};
use crate::{BootError, WindowState};
use crash::Crash;
use modes::Mode;
pub(crate) use modes::NavOp;

/// The `Setting` statics of `System::settings`, in the field order of `std::system::Settings`.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub(crate) struct EngineSettings {
    pub text_speed: StaticId,
    pub auto_delay: StaticId,
    pub auto_delay_per_char: StaticId,
    pub skip_unread: StaticId,
    pub master: StaticId,
    pub music: StaticId,
    pub sound: StaticId,
    pub voice: StaticId,
    pub voice_sustain: StaticId,
    pub wait_for_voice: StaticId,
    pub window_opacity: StaticId,
    pub fullscreen: StaticId,
    pub animations: StaticId,
}

/// The `System` value `main` returned.
#[derive(Clone, Debug)]
pub(crate) struct SystemConfig {
    pub title: Text,
    /// The design resolution, both sides at least 1.
    pub resolution: (u32, u32),
    /// `fn() -> RootPage`.
    pub start: Value,
    /// `fn(Narrative) -> Element`.
    pub game: Value,
    pub save_slots: u32,
    pub auto_slots: u32,
    pub backlog: u32,
    pub font: AssetId,
    /// The `TextStyle` of `System::text`, decoded by the text system once fonts are loaded.
    pub text: Value,
    pub settings: EngineSettings,
    pub bindings: Vec<Binding>,
}

impl SystemConfig {
    /// Decodes the `System` value by field index (`std::system::System`).
    pub fn decode(vm: &Vm, std: &StdTypes, v: &Value) -> Result<SystemConfig, BootError> {
        let v = match v {
            Value::Ref(r) => vm.load(r).map_err(|_| malformed("the value"))?,
            other => other.clone(),
        };
        let fields = std
            .fields(&v, StdTy::System)
            .map_err(|_| malformed("the value"))?;
        let field = |index: usize, name: &str| fields.get(index).ok_or_else(|| malformed(name));
        let count = |index: usize, name: &str| -> Result<u32, BootError> {
            decode_int(field(index, name)?, "System", name).map_err(|_| malformed(name))
        };
        let Value::Text(title) = field(0, "title")? else {
            return Err(malformed("title"));
        };
        let [w, h] =
            decode_size(std, field(1, "resolution")?).map_err(|_| malformed("resolution"))?;
        if w == 0 || h == 0 {
            return Err(BootError::Trap(
                "`System::resolution` must be at least 1×1".to_owned(),
            ));
        }
        let Value::Asset(font) = field(7, "font")? else {
            return Err(malformed("font"));
        };
        Ok(SystemConfig {
            title: title.clone(),
            resolution: (w, h),
            start: field(2, "start")?.clone(),
            game: field(3, "game")?.clone(),
            save_slots: count(4, "save_slots")?,
            auto_slots: count(5, "auto_slots")?,
            backlog: count(6, "backlog")?,
            font: *font,
            text: field(8, "text")?.clone(),
            settings: decode_settings(vm.program(), std, field(9, "settings")?)?,
            bindings: decode_bindings(std, field(10, "bindings")?).map_err(BootError::Trap)?,
        })
    }
}

fn malformed(field: &str) -> BootError {
    BootError::Trap(format!("`main` returned a malformed `System`: {field}"))
}

/// The 13 `Setting` handles of `std::system::Settings`.
fn decode_settings(
    program: &Program,
    std: &StdTypes,
    v: &Value,
) -> Result<EngineSettings, BootError> {
    let fields = std
        .fields(v, StdTy::Settings)
        .map_err(|_| malformed("settings"))?;
    let setting = |index: usize| -> Result<StaticId, BootError> {
        let id = fields
            .get(index)
            .and_then(|f| decode_handle(f).ok())
            .ok_or_else(|| malformed("settings"))?;
        let is_setting = usize::try_from(id.0)
            .ok()
            .and_then(|i| program.module().statics.get(i))
            .is_some_and(|s| s.kind == kagari_bytecode::StaticKind::Setting);
        if is_setting {
            Ok(id)
        } else {
            Err(malformed("settings"))
        }
    };
    Ok(EngineSettings {
        text_speed: setting(0)?,
        auto_delay: setting(1)?,
        auto_delay_per_char: setting(2)?,
        skip_unread: setting(3)?,
        master: setting(4)?,
        music: setting(5)?,
        sound: setting(6)?,
        voice: setting(7)?,
        voice_sustain: setting(8)?,
        wait_for_voice: setting(9)?,
        window_opacity: setting(10)?,
        fullscreen: setting(11)?,
        animations: setting(12)?,
    })
}

/// The system machine's state.
pub(crate) struct SystemState {
    /// The std import of every `ImportId`, bound at boot.
    pub imports: Bindings,
    /// The `System` value; `None` until `main` returned.
    pub config: Option<SystemConfig>,
    /// Settings, meta, read flags and seen sets.
    pub stores: Stores,
    /// The mode stack, bottom to top.
    pub modes: Vec<Mode>,
    pub crashed: Option<Crash>,
    /// Hold-skip is held.
    pub hold_skip: bool,
    /// A save is being replayed: no read flags, seen sets, meta writes, audio or autosaves.
    pub replaying: bool,
    /// The inputs pressed now.
    pub held: HashSet<HeldInput>,
    pub saves: SaveIndex,
    pub window: WindowState,
    /// The generation of the next game.
    pub next_generation: u64,
    /// The mode ids after the last `settle`, for the progress log.
    pub logged_modes: Vec<crate::ui::ModeId>,
    /// The lowercase hex SHA-256 of the bytecode, recorded in saves.
    pub script_hash: String,
}

impl SystemState {
    pub fn new(imports: Bindings, stores: Stores) -> SystemState {
        SystemState {
            imports,
            config: None,
            stores,
            modes: Vec::new(),
            crashed: None,
            hold_skip: false,
            replaying: false,
            held: HashSet::new(),
            saves: SaveIndex::default(),
            window: WindowState {
                title: String::new(),
                resolution: (1, 1),
                fullscreen: false,
                quit: false,
            },
            next_generation: 1,
            logged_modes: Vec::new(),
            script_hash: String::new(),
        }
    }

    /// The design resolution; `None` before `main` returned.
    pub fn resolution(&self) -> Option<(u32, u32)> {
        self.config.as_ref().map(|c| c.resolution)
    }

    /// The current value of a `Setting` static; `None` for any other static.
    pub fn setting_value(&self, id: StaticId) -> Option<&Value> {
        self.stores.settings.get(&id)
    }

    /// A setting of `System::settings`, picked by `f`.
    pub fn engine_setting(&self, f: impl FnOnce(&EngineSettings) -> StaticId) -> Option<&Value> {
        let id = f(&self.config.as_ref()?.settings);
        self.setting_value(id)
    }

    /// A `bool` setting; `default` before `main` returned.
    pub fn flag(&self, f: impl FnOnce(&EngineSettings) -> StaticId, default: bool) -> bool {
        match self.engine_setting(f) {
            Some(Value::Bool(b)) => *b,
            _ => default,
        }
    }

    /// A `Duration` setting in ms; `default` before `main` returned.
    pub fn duration_ms(&self, f: impl FnOnce(&EngineSettings) -> StaticId, default: u32) -> u32 {
        match self.engine_setting(f) {
            Some(Value::Adt(adt)) => match adt.fields.first() {
                Some(Value::Int(ms)) => u32::try_from(*ms).unwrap_or(default),
                _ => default,
            },
            _ => default,
        }
    }

    /// The `animations` setting; `true` before `main` returned.
    pub fn animations(&self) -> bool {
        self.flag(|s| s.animations, true)
    }

    /// The `window_opacity` setting as `f32`, clamped to 0..=1; 1.0 before `main` returned.
    pub fn window_opacity(&self) -> f32 {
        match self.engine_setting(|s| s.window_opacity) {
            Some(Value::Int(percent)) => ((*percent).clamp(0, 100) as f32 / 100.0).clamp(0.0, 1.0),
            _ => 1.0,
        }
    }

    /// Inserts into the seen-image set; on a new entry sets meta dirty and returns true.
    pub fn mark_seen_image(&mut self, id: AssetId) -> bool {
        let new = self.stores.seen_image.ids.insert(id);
        self.stores.dirty_meta |= new;
        new
    }

    /// The same for the heard set.
    pub fn mark_heard(&mut self, id: AssetId) -> bool {
        let new = self.stores.heard.ids.insert(id);
        self.stores.dirty_meta |= new;
        new
    }

    /// The top mode's kind.
    pub fn top(&self) -> Option<modes::ModeKind> {
        self.modes.last().map(|m| m.kind)
    }
}
