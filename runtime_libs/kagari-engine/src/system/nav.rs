//! `std::system::nav`: deferred navigation.

use kagari_vm::{Trap, Value, Vm};

use super::NavOp;
use crate::HostCtx;
use crate::saves;

import_table!(
    /// The `std::system::nav` imports.
    NavImport {
        Call = "std::system::nav::call",
        Dialog = "std::system::nav::dialog",
        Back = "std::system::nav::back",
        Goto = "std::system::nav::goto",
        Start = "std::system::nav::start",
        Load = "std::system::nav::load",
        Quit = "std::system::nav::quit",
    }
);

impl NavImport {
    pub fn call(self, ctx: &mut HostCtx, vm: &mut Vm, args: Vec<Value>) -> Result<Value, Trap> {
        use NavImport as I;
        let path = self.path();
        let arg = |i: usize| crate::values::arg(vm, path, &args, i);
        let op = match self {
            I::Call => NavOp::Call(arg(0)?),
            I::Dialog => NavOp::Dialog(arg(0)?),
            I::Back => NavOp::Back,
            I::Goto => NavOp::Goto(arg(0)?),
            I::Quit => NavOp::Quit,
            I::Start => {
                let program = ctx.state.program.clone();
                let entry = match arg(0)? {
                    Value::Func(f)
                        if program
                            .module()
                            .functions
                            .get(f.0 as usize)
                            .is_some_and(|func| func.flags.entry) =>
                    {
                        f
                    }
                    _ => {
                        return Err(Trap::host(
                            "`nav::start` needs a pointer to an `#[entry]` scene",
                        ));
                    }
                };
                let args = match arg(1)? {
                    Value::Tuple(items) => items.as_ref().clone(),
                    other => {
                        return Err(Trap::host(format!(
                            "`{path}` expected a tuple of arguments, found {}",
                            other.tag()
                        )));
                    }
                };
                let on_exit = arg(2)?;
                let seed = ctx.platform.random_seed();
                NavOp::Start {
                    entry,
                    args,
                    on_exit,
                    seed,
                }
            }
            I::Load => {
                let key = saves::decode_slot(ctx, &arg(0)?)?;
                let r = saves::load_slot(ctx, vm, key);
                return Ok(saves::load_result(ctx, r));
            }
        };
        ctx.state.pending_nav.push(op);
        Ok(Value::unit())
    }
}
