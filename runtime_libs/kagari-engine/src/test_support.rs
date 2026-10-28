//! Shared test helpers: compiling fixtures, a fake platform and a host that dispatches the
//! screen and audio imports.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use std::path::PathBuf;
use std::rc::Rc;
use std::sync::Arc;

use kagari_lang::{FilePath, MemoryFileSystem, RootDatabase, Severity, build, render_diagnostics};
use kagari_vm::{Host, HostFn, ImportId, Program, Trap, Value, Vm};

use crate::assets::AssetStore;
use crate::audio::AudioImport;
use crate::frame::Frame;
use crate::host::Bindings;
use crate::saves::Timestamp;
use crate::screen::ScreenImport;
use crate::{EngineState, HostCtx, Platform};

/// The fixture that interns every std type of `values/table.rs`.
pub(crate) const STD_TYPES: &str = include_str!("../tests/fixtures/std_types/main.kix");

/// The empty asset files every fixture compiles with.
const ASSETS: [&str; 12] = [
    "font.ttf",
    "a.png",
    "a.ogg",
    "v.ogg",
    "m.webm",
    "glow.wgsl",
    "sepia.wgsl",
    "blend.wgsl",
    "round.wgsl",
    "spots.wgsl",
    "bob.wgsl",
    "arc.wgsl",
];

/// Compiles `source` as `/game/main.kix`, with the empty [`ASSETS`]; panics on an error
/// diagnostic.
pub(crate) fn compile(source: &str) -> Vec<u8> {
    let mut files = rustc_hash::FxHashMap::default();
    files.insert(PathBuf::from("/game/main.kix"), source.to_owned());
    for asset in ASSETS {
        files.insert(PathBuf::from(format!("/game/{asset}")), String::new());
    }
    let db = RootDatabase::new(Arc::new(MemoryFileSystem(files)));
    let result = build(&db, db.program(FilePath::disk("/game/main.kix")));
    if result
        .diagnostics
        .iter()
        .any(|diag| diag.severity == Severity::Error)
    {
        panic!("{}", render_diagnostics(&db, &result.diagnostics, None));
    }
    result.bytes.clone().expect("bytecode")
}

/// The font every font asset reads as.
const TEST_FONT: &[u8] = include_bytes!("../../kagari-text/tests/fonts/kagari-test-cjk.otf");

/// A platform whose fonts are the test font and whose other assets are empty files lasting one
/// second.
pub(crate) struct FakePlatform;

impl Platform for FakePlatform {
    fn read_store(&mut self, _name: &str) -> std::io::Result<Option<Vec<u8>>> {
        Ok(None)
    }

    fn write_store(&mut self, _name: &str, _bytes: &[u8]) -> std::io::Result<()> {
        Ok(())
    }

    fn delete_store(&mut self, _name: &str) -> std::io::Result<()> {
        Ok(())
    }

    /// Font assets get the CJK test font of `kagari-text`; every other asset is empty.
    fn read_asset(&mut self, path: &str) -> std::io::Result<Option<Vec<u8>>> {
        if path.ends_with(".ttf") || path.ends_with(".otf") {
            return Ok(Some(TEST_FONT.to_vec()));
        }
        Ok(Some(Vec::new()))
    }

    fn audio_duration_ms(&mut self, _path: &str, _bytes: &[u8]) -> Option<u32> {
        Some(1000)
    }

    fn render_thumbnail(
        &mut self,
        _frame: &Frame,
        _assets: &AssetStore,
        _width: u32,
        _height: u32,
    ) -> Option<Vec<u8>> {
        None
    }

    fn local_time(&self) -> Timestamp {
        Timestamp {
            year: 2026,
            month: 1,
            day: 2,
            hour: 3,
            minute: 4,
            second: 5,
        }
    }

    fn random_seed(&mut self) -> u64 {
        7
    }

    fn set_title(&mut self, _title: &str) {}
}

/// A VM and engine state over the compiled `source`, with no imports bound (for `TestHost`).
pub(crate) fn engine(source: &str) -> (Vm, EngineState) {
    let program: Rc<Program> = Program::load(&compile(source)).unwrap();
    state_for(program, Bindings::default())
}

/// A VM and engine state over the compiled `source`, with every import bound, so `HostCtx`
/// dispatches them; panics on an import the engine does not provide yet.
pub(crate) fn bound_engine(source: &str) -> (Vm, EngineState) {
    let program: Rc<Program> = Program::load(&compile(source)).unwrap();
    let bindings = Bindings::new(&program).unwrap();
    state_for(program, bindings)
}

fn state_for(program: Rc<Program>, bindings: Bindings) -> (Vm, EngineState) {
    let mut vm = Vm::new(program.clone());
    let state = EngineState::new(&mut vm, program, bindings);
    (vm, state)
}

/// Dispatches the screen and audio imports; any other import traps.
pub(crate) struct TestHost<'a>(pub HostCtx<'a>);

impl Host for TestHost<'_> {
    fn import(&mut self, vm: &mut Vm, import: ImportId, args: Vec<Value>) -> Result<Value, Trap> {
        let program = vm.program().clone();
        let path = program.string(program.module().imports[import.0 as usize].path);
        if let Some(import) = ScreenImport::from_path(path) {
            return import.call(&mut self.0, vm, args);
        }
        if let Some(import) = AudioImport::from_path(path) {
            return import.call(&mut self.0, vm, args);
        }
        Err(Trap::host(format!("unexpected import `{path}`")))
    }

    fn call_host_fn(
        &mut self,
        _vm: &mut Vm,
        _f: &HostFn,
        _args: Vec<Value>,
    ) -> Result<Value, Trap> {
        Err(Trap::host("unexpected host fn"))
    }
}
