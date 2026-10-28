#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use kagari_vm::{AdtTy, Program, Vm};

use super::{STD_ADTS, StdTy, StdTypes};
use crate::test_support::{STD_TYPES, compile};

/// `STD_ADTS` matches the std declarations as a compiled program interns them.
#[test]
fn std_table_matches_the_compiled_std() {
    let program = Program::load(&compile(STD_TYPES)).unwrap();
    let module = program.module();
    for decl in &STD_ADTS {
        let found: Vec<_> = module
            .types
            .iter()
            .filter(|ty| program.string(ty.path) == decl.path)
            .collect();
        let [ty] = found.as_slice() else {
            panic!("{} not interned: add a keep(…) for it", decl.path);
        };
        assert_eq!(ty.is_enum, decl.is_enum, "{}: is_enum", decl.path);
        let variants: Vec<(&str, Vec<Option<&str>>)> = ty
            .variants
            .iter()
            .map(|v| {
                let fields = v
                    .fields
                    .iter()
                    .map(|f| f.map(|name| program.string(name)))
                    .collect();
                (program.string(v.name), fields)
            })
            .collect();
        let expected: Vec<(&str, Vec<Option<&str>>)> = decl
            .variants
            .iter()
            .map(|v| (v.name, v.fields.to_vec()))
            .collect();
        assert_eq!(variants, expected, "{}: variants", decl.path);
    }
}

#[test]
fn std_types_resolve_to_module_types() {
    let program = Program::load(&compile(STD_TYPES)).unwrap();
    let mut vm = Vm::new(program.clone());
    let std = StdTypes::resolve(vm.types_mut());
    let option = program.find_type("std::Option").unwrap();
    assert_eq!(std.ty(StdTy::Option), AdtTy::Module(option));
    let some = std.some(kagari_vm::Value::Int(3));
    assert!(matches!(
        std.option(&some),
        Ok(Some(kagari_vm::Value::Int(3)))
    ));
    assert!(matches!(std.option(&std.none()), Ok(None)));
}
