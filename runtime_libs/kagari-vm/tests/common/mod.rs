#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic, dead_code)]

use std::path::PathBuf;
use std::rc::Rc;
use std::sync::Arc;

use kagari_bytecode::{
    FnFlags, FnKind, Function, Import, Instr, IntType, Module, Options, StrId, encode,
};
use kagari_lang::{FilePath, MemoryFileSystem, RootDatabase, Severity, build, render_diagnostics};
use kagari_vm::{FuncId, Host, HostFn, ImportId, Program, Trap, Value, Vm};
use rustc_hash::FxHashMap;

/// The settings statics, the font, `main` (which first calls `keep()`), `Title` and
/// `GameView` of `core/kagari-lang/tests/bytecode/001__arith/main.kix`.
const TAIL: &str = r#"
static TEXT_SPEED: Setting<Duration> = setting(ms(30));
static AUTO_DELAY: Setting<Duration> = setting(ms(1300));
static AUTO_DELAY_PER_CHAR: Setting<Duration> = setting(ms(60));
static SKIP_UNREAD: Setting<bool> = setting(false);
static MASTER: Setting<Volume> = setting(volume(100));
static MUSIC: Setting<Volume> = setting(volume(60));
static SOUND: Setting<Volume> = setting(volume(100));
static VOICE: Setting<Volume> = setting(volume(100));
static VOICE_SUSTAIN: Setting<bool> = setting(false);
static WAIT_FOR_VOICE: Setting<bool> = setting(true);
static WINDOW_OPACITY: Setting<Int<0, 100>> = setting(80);
static FULLSCREEN: Setting<bool> = setting(false);
static ANIMATIONS: Setting<bool> = setting(true);
const FONT: FontAsset = asset("font.ttf");

fn main() -> System {
    keep();
    System {
        title: "Arithmetic",
        resolution: size(1280, 720),
        start: Title,
        game: GameView,
        save_slots: 20,
        auto_slots: 3,
        backlog: 200,
        font: FONT,
        text: text_style(26, WHITE),
        settings: Settings {
            text_speed: TEXT_SPEED,
            auto_delay: AUTO_DELAY,
            auto_delay_per_char: AUTO_DELAY_PER_CHAR,
            skip_unread: SKIP_UNREAD,
            master: MASTER,
            music: MUSIC,
            sound: SOUND,
            voice: VOICE,
            voice_sustain: VOICE_SUSTAIN,
            wait_for_voice: WAIT_FOR_VOICE,
            window_opacity: WINDOW_OPACITY,
            fullscreen: FULLSCREEN,
            animations: ANIMATIONS,
        },
        bindings: default_bindings(),
    }
}

fn Title() -> RootPage {
    RootPage(text!(t!["Title"]))
}

fn GameView(game: Narrative) -> Element {
    game_screen!(game = game)
}
"#;

/// Compiles `items` into a program. `items` must define `fn keep()`, which calls every
/// function under test: only code reachable from `main` or an entry scene is emitted.
pub fn compile(items: &str) -> Rc<Program> {
    let source = format!(
        "use std::narrative::text::text_style;\nuse std::screen::size;\nuse std::system::default_bindings;\n\n{items}\n{TAIL}"
    );
    let mut files = FxHashMap::default();
    files.insert(PathBuf::from("/game/main.kix"), source);
    files.insert(PathBuf::from("/game/font.ttf"), String::new());
    let db = RootDatabase::new(Arc::new(MemoryFileSystem(files)));
    let result = build(&db, db.program(FilePath::disk("/game/main.kix")));
    if result
        .diagnostics
        .iter()
        .any(|diag| diag.severity == Severity::Error)
    {
        panic!("{}", render_diagnostics(&db, &result.diagnostics, None));
    }
    let bytes = result.bytes.as_ref().expect("bytecode");
    Program::load(bytes).unwrap()
}

/// A function of a hand-built module.
pub struct HandFn {
    pub path: &'static str,
    pub kind: FnKind,
    pub params: u16,
    pub regs: u16,
    pub closure: bool,
    pub code: Vec<Instr>,
}

impl HandFn {
    pub fn new(path: &'static str, params: u16, regs: u16, code: Vec<Instr>) -> HandFn {
        HandFn {
            path,
            kind: FnKind::Fn,
            params,
            regs,
            closure: false,
            code,
        }
    }
}

/// A module built directly: int types after the mandatory `i128`, imports `(path, params)`
/// of kind `fn`, the functions (ids in order), then a trivial `main`.
pub fn hand(
    int_types: &[IntType],
    imports: &[(&str, u16)],
    functions: Vec<HandFn>,
    max_call_depth: u32,
) -> Rc<Program> {
    let mut module = Module::default();
    let mut string = |s: &str| {
        module.strings.push(s.to_owned());
        StrId(u32::try_from(module.strings.len() - 1).unwrap())
    };
    let import_paths: Vec<StrId> = imports.iter().map(|(p, _)| string(p)).collect();
    let fn_paths: Vec<StrId> = functions.iter().map(|f| string(f.path)).collect();
    let main_path = string("t::main");
    module.int_types.push(IntType::I128);
    module.int_types.extend_from_slice(int_types);
    module.imports = imports
        .iter()
        .zip(import_paths)
        .map(|(&(_, params), path)| Import {
            path,
            kind: FnKind::Fn,
            params,
        })
        .collect();
    for (f, path) in functions.into_iter().zip(fn_paths) {
        module.functions.push(Function {
            path,
            kind: f.kind,
            flags: FnFlags {
                closure: f.closure,
                ..FnFlags::default()
            },
            params: f.params,
            regs: f.regs,
            code: f.code,
        });
    }
    module.functions.push(Function {
        path: main_path,
        kind: FnKind::Fn,
        flags: FnFlags {
            main: true,
            ..FnFlags::default()
        },
        params: 0,
        regs: 1,
        code: vec![
            Instr::Tuple {
                dst: 0,
                first: 0,
                count: 0,
            },
            Instr::Ret { src: 0 },
        ],
    });
    module.options = Options { max_call_depth };
    Program::load(&encode(&module)).unwrap()
}

/// Records imports; `t::reenter` calls `reenter_target` back through the VM, and `t::fresh`
/// calls it `fresh_runs` times inside `Vm::with_fresh_budget`.
#[derive(Default)]
pub struct TestHost {
    pub calls: Vec<(String, Vec<Value>)>,
    pub reenter_target: Option<FuncId>,
    pub fresh_runs: u32,
}

impl Host for TestHost {
    fn import(&mut self, vm: &mut Vm, import: ImportId, args: Vec<Value>) -> Result<Value, Trap> {
        let program = vm.program().clone();
        let path = program
            .string(program.module().imports[import.0 as usize].path)
            .to_owned();
        self.calls.push((path.clone(), args));
        if path == "t::reenter" {
            let target = self.reenter_target.expect("a reenter target");
            return vm.call(self, target, vec![]);
        }
        if path == "t::fresh" {
            let target = self.reenter_target.expect("a reenter target");
            let runs = self.fresh_runs;
            return vm.with_fresh_budget(|vm| {
                for _ in 0..runs {
                    vm.call(self, target, vec![])?;
                }
                Ok(Value::unit())
            });
        }
        Ok(Value::unit())
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

pub fn func(program: &Program, path: &str) -> FuncId {
    program
        .find_function(path)
        .unwrap_or_else(|| panic!("no function `{path}`"))
}

pub fn int(value: &Value) -> i128 {
    match value {
        Value::Int(i) => *i,
        other => panic!("expected an Int, found {other:?}"),
    }
}
