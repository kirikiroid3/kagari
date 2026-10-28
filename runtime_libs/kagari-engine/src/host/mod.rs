//! Host dispatch: every `ImportId` bound once to a std import, and the `Host` the VM calls.

use kagari_vm::{Host, HostFn, ImportId, Program, Trap, Value, Vm};

use crate::audio::AudioImport;
use crate::saves::SavesImport;
use crate::screen::ScreenImport;
use crate::story::narrative::NarrativeImport;
use crate::system::imports::SystemImport;
use crate::system::nav::NavImport;
use crate::ui::{self, UiImport};
use crate::{BootError, HostCtx};

/// A std import, by module.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub(crate) enum StdImport {
    Screen(ScreenImport),
    Audio(AudioImport),
    Narrative(NarrativeImport),
    System(SystemImport),
    Nav(NavImport),
    Saves(SavesImport),
    Ui(UiImport),
}

impl StdImport {
    /// The import whose std path is `path`; the first module that accepts it wins.
    fn from_path(path: &str) -> Option<StdImport> {
        ScreenImport::from_path(path)
            .map(StdImport::Screen)
            .or_else(|| AudioImport::from_path(path).map(StdImport::Audio))
            .or_else(|| NarrativeImport::from_path(path).map(StdImport::Narrative))
            .or_else(|| SystemImport::from_path(path).map(StdImport::System))
            .or_else(|| NavImport::from_path(path).map(StdImport::Nav))
            .or_else(|| SavesImport::from_path(path).map(StdImport::Saves))
            .or_else(|| UiImport::from_path(path).map(StdImport::Ui))
    }

    fn call(self, ctx: &mut HostCtx, vm: &mut Vm, args: Vec<Value>) -> Result<Value, Trap> {
        match self {
            StdImport::Screen(import) => import.call(ctx, vm, args),
            StdImport::Audio(import) => import.call(ctx, vm, args),
            StdImport::Narrative(import) => import.call(ctx, vm, args),
            StdImport::System(import) => import.call(ctx, vm, args),
            StdImport::Nav(import) => import.call(ctx, vm, args),
            StdImport::Saves(import) => import.call(ctx, vm, args),
            StdImport::Ui(import) => import.call(ctx, vm, args),
        }
    }
}

/// The std import of every `ImportId`, indexed by `ImportId.0`.
#[derive(Default)]
pub(crate) struct Bindings {
    imports: Vec<StdImport>,
}

impl Bindings {
    /// Binds every import of `program`; a path no std module provides fails the boot.
    pub fn new(program: &Program) -> Result<Bindings, BootError> {
        let imports = program
            .module()
            .imports
            .iter()
            .map(|import| {
                let path = program.string(import.path);
                StdImport::from_path(path)
                    .ok_or_else(|| BootError::UnsupportedImport(path.to_owned()))
            })
            .collect::<Result<_, _>>()?;
        Ok(Bindings { imports })
    }

    pub fn get(&self, id: ImportId) -> Option<StdImport> {
        self.imports.get(usize::try_from(id.0).ok()?).copied()
    }
}

impl Host for HostCtx<'_> {
    fn import(&mut self, vm: &mut Vm, import: ImportId, args: Vec<Value>) -> Result<Value, Trap> {
        let Some(bound) = self.state.system.imports.get(import) else {
            return Err(Trap::host(format!("unknown import {}", import.0)));
        };
        bound.call(self, vm, args)
    }

    fn call_host_fn(&mut self, vm: &mut Vm, f: &HostFn, args: Vec<Value>) -> Result<Value, Trap> {
        ui::call_host_fn(self, vm, f, args)
    }
}
