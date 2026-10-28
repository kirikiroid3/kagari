//! `std::system`: settings, the seen sets and the system music.

use kagari_vm::{StaticId, Trap, Value, Vm};

use crate::store::value::check_storable;
use crate::ui::SourceKey;
use crate::ui::imports::deref;
use crate::values::{decode_asset, decode_audio_fade, decode_loop_spec, decode_volume};
use crate::{EngineState, HostCtx};

import_table!(
    /// The `std::system` imports.
    SystemImport {
        SettingGet = "std::system::Setting::get",
        SettingSet = "std::system::Setting::set",
        SettingInit = "std::system::setting",
        SeenImage = "std::system::seen_image",
        Heard = "std::system::heard",
        Watched = "std::system::watched",
        SetMusic = "std::system::set_music",
    }
);

/// Applies the volume settings to the audio model and the fullscreen setting to the window.
pub(crate) fn apply_engine_settings(state: &mut EngineState) -> Result<(), Trap> {
    let Some(config) = &state.system.config else {
        return Ok(());
    };
    let s = config.settings;
    let volume = |id: StaticId| -> Result<crate::values::Volume, Trap> {
        let v = state
            .system
            .setting_value(id)
            .ok_or_else(|| Trap::host("a volume setting has no value"))?;
        decode_volume(&state.std, v)
    };
    let (master, music, sound, voice) = (
        volume(s.master)?,
        volume(s.music)?,
        volume(s.sound)?,
        volume(s.voice)?,
    );
    state.audio.set_volumes(master, music, sound, voice);
    state.system.window.fullscreen = state.system.flag(|s| s.fullscreen, false);
    Ok(())
}

fn handle(vm: &Vm, path: &str, v: Option<&Value>) -> Result<StaticId, Trap> {
    match deref(vm, v.cloned().unwrap_or(Value::Uninit))? {
        Value::Handle(id) => Ok(id),
        other => Err(Trap::host(format!(
            "`{path}` expected a handle, found {}",
            other.tag()
        ))),
    }
}

impl SystemImport {
    pub fn call(self, ctx: &mut HostCtx, vm: &mut Vm, args: Vec<Value>) -> Result<Value, Trap> {
        use SystemImport as I;
        let path = self.path();
        let arg = |i: usize| crate::values::arg(vm, path, &args, i);
        match self {
            I::SettingInit => Err(Trap::host(
                "`meta(…)` and `setting(…)` only initialize statics",
            )),
            I::SettingGet => {
                let id = handle(vm, path, args.first())?;
                ctx.state.ui.track(SourceKey::Setting(id));
                ctx.state
                    .system
                    .setting_value(id)
                    .cloned()
                    .ok_or_else(|| Trap::host(format!("`{path}`: not a `Setting` static")))
            }
            I::SettingSet => {
                let id = handle(vm, path, args.first())?;
                let value = arg(1)?;
                check_storable(&value, "Setting")?;
                let stores = &mut ctx.state.system.stores;
                if !stores.settings.contains_key(&id) {
                    return Err(Trap::host(format!("`{path}`: not a `Setting` static")));
                }
                stores.settings.insert(id, value);
                stores.dirty_settings = true;
                ctx.state.ui.invalidate(SourceKey::Setting(id));
                let affects_engine = ctx.state.system.config.as_ref().is_some_and(|c| {
                    let s = c.settings;
                    [s.master, s.music, s.sound, s.voice, s.fullscreen].contains(&id)
                });
                if affects_engine {
                    apply_engine_settings(ctx.state)?;
                }
                Ok(Value::unit())
            }
            I::SeenImage | I::Heard | I::Watched => {
                let id = decode_asset(&arg(0)?)?;
                let stores = &ctx.state.system.stores;
                let (key, seen) = match self {
                    I::SeenImage => (
                        SourceKey::SeenImage(id),
                        stores.seen_image.ids.contains(&id),
                    ),
                    I::Heard => (SourceKey::Heard(id), stores.heard.ids.contains(&id)),
                    _ => (SourceKey::Watched(id), stores.watched.ids.contains(&id)),
                };
                ctx.state.ui.track(key);
                Ok(Value::Bool(seen))
            }
            I::SetMusic => {
                let track = arg(0)?;
                let fade = arg(1)?;
                let state = &mut *ctx.state;
                let track = state
                    .std
                    .option(&track)?
                    .map(|l| decode_loop_spec(&state.std, &state.program, l))
                    .transpose()?;
                let fade = decode_audio_fade(&state.std, &fade)?;
                state.audio.set_system_music(track, fade);
                Ok(Value::unit())
            }
        }
    }
}
