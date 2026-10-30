//! ADT types by path: the module's, plus synthetic entries for std types the module never
//! interned (the engine builds values of those too).

use std::rc::Rc;

use rustc_hash::FxHashMap;

use crate::program::Program;
use crate::value::AdtTy;

/// Module types plus synthetic entries for std types the module did not intern.
#[derive(Debug)]
pub struct TypeTable {
    program: Rc<Program>,
    synthetic: Vec<Synthetic>,
    by_path: FxHashMap<Box<str>, u32>,
}

/// A synthetic type: its path, variant names and whether it is an enum.
type Synthetic = (Box<str>, Box<[Box<str>]>, bool);

const UNKNOWN: &str = "<unknown>";

impl TypeTable {
    pub(crate) fn new(program: Rc<Program>) -> TypeTable {
        TypeTable {
            program,
            synthetic: Vec::new(),
            by_path: FxHashMap::default(),
        }
    }

    /// The module TypeId of `path` when interned, else a stable synthetic id registered on
    /// first request, with the given variant names and enum flag (the first registration's
    /// stand).
    pub fn resolve(&mut self, path: &str, variants: &[&str], is_enum: bool) -> AdtTy {
        if let Some(ty) = self.program.find_type(path) {
            return AdtTy::Module(ty);
        }
        if let Some(&id) = self.by_path.get(path) {
            return AdtTy::Synthetic(id);
        }
        let id = u32::try_from(self.synthetic.len()).unwrap_or(u32::MAX);
        self.synthetic.push((
            path.into(),
            variants.iter().map(|&v| v.into()).collect(),
            is_enum,
        ));
        self.by_path.insert(path.into(), id);
        AdtTy::Synthetic(id)
    }

    /// Whether `ty` is an enum (rather than a struct); `false` when out of range.
    pub fn is_enum(&self, ty: AdtTy) -> bool {
        match ty {
            AdtTy::Module(id) => self
                .program
                .module()
                .types
                .get(id.0 as usize)
                .is_some_and(|adt| adt.is_enum),
            AdtTy::Synthetic(id) => self
                .synthetic
                .get(id as usize)
                .is_some_and(|(_, _, is_enum)| *is_enum),
        }
    }

    /// The path of `ty`; `"<unknown>"` when out of range.
    pub fn path(&self, ty: AdtTy) -> &str {
        match ty {
            AdtTy::Module(id) => match self.program.module().types.get(id.0 as usize) {
                Some(adt) => self.program.string(adt.path),
                None => UNKNOWN,
            },
            AdtTy::Synthetic(id) => self
                .synthetic
                .get(id as usize)
                .map_or(UNKNOWN, |(path, _, _)| path),
        }
    }

    /// The name of variant `variant` of `ty`; `"<unknown>"` when out of range.
    pub fn variant_name(&self, ty: AdtTy, variant: u32) -> &str {
        match ty {
            AdtTy::Module(id) => match self
                .program
                .module()
                .types
                .get(id.0 as usize)
                .and_then(|adt| adt.variants.get(variant as usize))
            {
                Some(v) => self.program.string(v.name),
                None => UNKNOWN,
            },
            AdtTy::Synthetic(id) => self
                .synthetic
                .get(id as usize)
                .and_then(|(_, variants, _)| variants.get(variant as usize))
                .map_or(UNKNOWN, |name| name),
        }
    }

    /// The definition of `ty` as paths and names: whether it is an enum, and per variant its
    /// name and, for a module type, its field names (`None` for positional fields). `None`
    /// when `ty` is out of range.
    pub(crate) fn shape(&self, ty: AdtTy) -> Option<TypeShape> {
        match ty {
            AdtTy::Module(id) => {
                let adt = self.program.module().types.get(id.0 as usize)?;
                Some(TypeShape {
                    is_enum: adt.is_enum,
                    variants: adt
                        .variants
                        .iter()
                        .map(|v| self.program.string(v.name).to_owned())
                        .collect(),
                    fields: Some(
                        adt.variants
                            .iter()
                            .map(|v| {
                                v.fields
                                    .iter()
                                    .map(|f| f.map(|f| self.program.string(f).to_owned()))
                                    .collect()
                            })
                            .collect(),
                    ),
                })
            }
            AdtTy::Synthetic(id) => {
                let (_, variants, is_enum) = self.synthetic.get(id as usize)?;
                Some(TypeShape {
                    is_enum: *is_enum,
                    variants: variants.iter().map(|v| v.to_string()).collect(),
                    fields: None,
                })
            }
        }
    }
}

/// What a type is made of, by name: two programs agree on a type when its shapes do.
#[derive(Debug, PartialEq, Eq)]
pub(crate) struct TypeShape {
    pub is_enum: bool,
    pub variants: Vec<String>,
    /// Per variant, the field names; `None` for a synthetic type (the engine registered only
    /// its variant names).
    pub fields: Option<Vec<Vec<Option<String>>>>,
}

impl TypeShape {
    /// Whether values of one may stand for values of the other: the same kind and variants,
    /// and the same fields where both know them.
    pub(crate) fn agrees(&self, other: &TypeShape) -> bool {
        self.is_enum == other.is_enum
            && self.variants == other.variants
            && match (&self.fields, &other.fields) {
                (Some(a), Some(b)) => a == b,
                _ => true,
            }
    }
}
