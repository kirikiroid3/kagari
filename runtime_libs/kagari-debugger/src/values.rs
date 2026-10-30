//! Story values between the VM, the story graph ([`kagari_debug::Value`], which stores values
//! the way saves do) and the protocol's display form.

use std::rc::Rc;

use kagari_debug as kd;
use kagari_vm::{
    Adt, AdtTy, AssetId, Closure, Decimal, FuncId, Program, Proj, Rat, RefRoot, StaticId, Text,
    TypeTable, Value,
};

use crate::protocol::Var;

/// What names a value refers to: type, asset, static and function paths.
#[derive(Clone, Copy)]
pub struct Names<'a> {
    pub program: &'a Program,
    pub types: &'a TypeTable,
}

impl<'a> Names<'a> {
    pub fn asset(&self, id: AssetId) -> &'a str {
        self.program
            .module()
            .assets
            .get(id.0 as usize)
            .map_or("", |a| self.program.string(a.path))
    }

    pub fn static_path(&self, id: StaticId) -> &'a str {
        self.program
            .module()
            .statics
            .get(id.0 as usize)
            .map_or("", |s| self.program.string(s.path))
    }

    pub fn function(&self, id: FuncId) -> &'a str {
        self.program.function_path(id)
    }

    /// The field names of a module type's variant; `None` for positional fields.
    fn fields(&self, ty: AdtTy, variant: u32) -> Vec<Option<&'a str>> {
        let AdtTy::Module(ty) = ty else {
            return Vec::new();
        };
        self.program
            .module()
            .types
            .get(ty.0 as usize)
            .and_then(|t| t.variants.get(variant as usize))
            .map(|v| {
                v.fields
                    .iter()
                    .map(|f| f.map(|name| self.program.string(name)))
                    .collect()
            })
            .unwrap_or_default()
    }
}

/// The value behind a reference (a cell, with its projections); host parts and broken paths
/// give `None`.
pub fn deref(v: &Value) -> Option<Value> {
    let Value::Ref(r) = v else {
        return Some(v.clone());
    };
    let RefRoot::Cell(cell) = &r.root else {
        return None;
    };
    let mut current = cell.try_borrow().ok()?.clone();
    for proj in r.path.iter() {
        current = match (proj, &current) {
            (Proj::Field(i), Value::Tuple(items)) => items.get(usize::from(*i))?.clone(),
            (Proj::Field(i), Value::Adt(adt)) => adt.fields.get(usize::from(*i))?.clone(),
            (Proj::Index(i), Value::Array(items) | Value::Vec(items)) => {
                items.get(usize::try_from(*i).ok()?)?.clone()
            }
            _ => return None,
        };
    }
    Some(current)
}

/// The graph's form of a value; `None` for what the graph cannot hold.
pub fn to_graph(names: Names<'_>, v: &Value) -> Option<kd::Value> {
    Some(match v {
        Value::Bool(b) => kd::Value::Bool(*b),
        Value::Int(i) => kd::Value::Int(*i),
        Value::Decimal(d) => kd::Value::Decimal {
            mantissa: d.mantissa(),
            scale: d.scale(),
        },
        Value::Rat(r) => kd::Value::Rat {
            num: r.numer(),
            den: r.denom(),
        },
        Value::String(s) => kd::Value::String(s.to_string()),
        Value::Text(t) => kd::Value::Text(text_to_graph(names, t)?),
        Value::Tuple(items) => kd::Value::Tuple(items_to_graph(names, items)?),
        Value::Array(items) => kd::Value::Array(items_to_graph(names, items)?),
        Value::Vec(items) => kd::Value::Vec(items_to_graph(names, items)?),
        Value::Adt(adt) => kd::Value::Adt {
            ty: names.types.path(adt.ty).to_owned(),
            variant: names.types.variant_name(adt.ty, adt.variant).to_owned(),
            fields: items_to_graph(names, &adt.fields)?,
        },
        Value::Asset(id) => kd::Value::Asset(names.asset(*id).to_owned()),
        Value::Handle(id) => kd::Value::Handle(names.static_path(*id).to_owned()),
        Value::Func(f) => kd::Value::Fn(names.function(*f).to_owned()),
        Value::Closure(c) if c.captures.is_empty() => {
            kd::Value::Closure(names.function(c.func).to_owned())
        }
        Value::Ref(_) => return to_graph(names, &deref(v)?),
        Value::Host(h) if h.0.type_name() == "Screen" => kd::Value::Screen,
        Value::Uninit | Value::Closure(_) | Value::Host(_) | Value::HostFn(_) => return None,
    })
}

fn items_to_graph(names: Names<'_>, items: &[Value]) -> Option<Vec<kd::Value>> {
    items.iter().map(|v| to_graph(names, v)).collect()
}

fn text_to_graph(names: Names<'_>, t: &Text) -> Option<kd::Text> {
    Some(match t {
        Text::Plain(s) => kd::Text::Plain(s.to_string()),
        Text::Ruby(p) => kd::Text::Ruby {
            base: Box::new(text_to_graph(names, &p.0)?),
            reading: Box::new(text_to_graph(names, &p.1)?),
        },
        Text::MonoRuby(p) => kd::Text::MonoRuby {
            base: Box::new(text_to_graph(names, &p.0)?),
            readings: p
                .1
                .iter()
                .map(|r| text_to_graph(names, r))
                .collect::<Option<_>>()?,
        },
        Text::Effect(p) => kd::Text::Effect {
            base: Box::new(text_to_graph(names, &p.0)?),
            effect: Box::new(to_graph(names, &p.1)?),
        },
        Text::Concat(items) => kd::Text::Concat(
            items
                .iter()
                .map(|t| text_to_graph(names, t))
                .collect::<Option<_>>()?,
        ),
    })
}

/// The VM value of a graph value; `None` when the program lacks a type, variant, asset, static
/// or function it names, or for a screen.
pub fn from_graph(program: &Program, v: &kd::Value) -> Option<Value> {
    let items = |items: &[kd::Value]| -> Option<Rc<Vec<Value>>> {
        Some(Rc::new(
            items
                .iter()
                .map(|v| from_graph(program, v))
                .collect::<Option<_>>()?,
        ))
    };
    let module = program.module();
    Some(match v {
        kd::Value::Bool(b) => Value::Bool(*b),
        kd::Value::Int(i) => Value::Int(*i),
        kd::Value::Decimal { mantissa, scale } => Value::Decimal(Decimal::new(*mantissa, *scale)),
        kd::Value::Rat { num, den } => Value::Rat(Rat::new(*num, *den)?),
        kd::Value::String(s) => Value::String(Rc::from(s.as_str())),
        kd::Value::Text(t) => Value::Text(text_from_graph(program, t)?),
        kd::Value::Tuple(xs) => Value::Tuple(items(xs)?),
        kd::Value::Array(xs) => Value::Array(items(xs)?),
        kd::Value::Vec(xs) => Value::Vec(items(xs)?),
        kd::Value::Adt {
            ty,
            variant,
            fields,
        } => {
            let id = program.find_type(ty)?;
            let index = module
                .types
                .get(id.0 as usize)?
                .variants
                .iter()
                .position(|v| program.string(v.name) == variant)?;
            Value::Adt(Rc::new(Adt {
                ty: AdtTy::Module(id),
                variant: u32::try_from(index).ok()?,
                fields: Rc::try_unwrap(items(fields)?).unwrap_or_else(|rc| (*rc).clone()),
            }))
        }
        kd::Value::Asset(path) => Value::Asset(AssetId(index_of(
            module.assets.iter().map(|a| program.string(a.path)),
            path,
        )?)),
        kd::Value::Handle(path) => Value::Handle(StaticId(index_of(
            module.statics.iter().map(|s| program.string(s.path)),
            path,
        )?)),
        kd::Value::Fn(path) => Value::Func(program.find_function(path)?),
        kd::Value::Closure(path) => Value::Closure(Rc::new(Closure {
            func: program.find_function(path)?,
            captures: Vec::new(),
        })),
        kd::Value::Screen => return None,
    })
}

fn index_of<'a>(mut names: impl Iterator<Item = &'a str>, name: &str) -> Option<u32> {
    u32::try_from(names.position(|n| n == name)?).ok()
}

fn text_from_graph(program: &Program, t: &kd::Text) -> Option<Text> {
    Some(match t {
        kd::Text::Plain(s) => Text::Plain(Rc::from(s.as_str())),
        kd::Text::Ruby { base, reading } => Text::Ruby(Rc::new((
            text_from_graph(program, base)?,
            text_from_graph(program, reading)?,
        ))),
        kd::Text::MonoRuby { base, readings } => Text::MonoRuby(Rc::new((
            text_from_graph(program, base)?,
            readings
                .iter()
                .map(|r| text_from_graph(program, r))
                .collect::<Option<_>>()?,
        ))),
        kd::Text::Effect { base, effect } => Text::Effect(Rc::new((
            text_from_graph(program, base)?,
            from_graph(program, effect)?,
        ))),
        kd::Text::Concat(items) => Text::Concat(Rc::new(
            items
                .iter()
                .map(|t| text_from_graph(program, t))
                .collect::<Option<_>>()?,
        )),
    })
}

/// The plain characters of a text: the base of rubies and effects, in order.
pub fn plain(t: &Text) -> String {
    let mut out = String::new();
    write_plain(t, &mut out);
    out
}

fn write_plain(t: &Text, out: &mut String) {
    match t {
        Text::Plain(s) => out.push_str(s),
        Text::Ruby(p) => write_plain(&p.0, out),
        Text::MonoRuby(p) => write_plain(&p.0, out),
        Text::Effect(p) => write_plain(&p.0, out),
        Text::Concat(items) => items.iter().for_each(|t| write_plain(t, out)),
    }
}

/// A text with its formatting: `{base|reading}` for a ruby, `{base|r1|r2}` for a mono ruby,
/// `[base]<effect>` for an effect.
pub fn rich(names: Names<'_>, t: &Text) -> String {
    let mut out = String::new();
    write_rich(names, t, &mut out);
    out
}

fn write_rich(names: Names<'_>, t: &Text, out: &mut String) {
    match t {
        Text::Plain(s) => out.push_str(s),
        Text::Ruby(p) => {
            out.push('{');
            write_rich(names, &p.0, out);
            out.push('|');
            write_rich(names, &p.1, out);
            out.push('}');
        }
        Text::MonoRuby(p) => {
            out.push('{');
            write_rich(names, &p.0, out);
            for reading in &p.1 {
                out.push('|');
                write_rich(names, reading, out);
            }
            out.push('}');
        }
        Text::Effect(p) => {
            out.push('[');
            write_rich(names, &p.0, out);
            out.push_str("]<");
            out.push_str(&display(names, &p.1));
            out.push('>');
        }
        Text::Concat(items) => items.iter().for_each(|t| write_rich(names, t, out)),
    }
}

/// A value as Kagari source would write it, roughly: `true`, `3`, `"s"`, `t!["…"]`, `(a, b)`,
/// `crate::Pick::B`, `crate::Point { x: 1, y: 2 }`.
pub fn display(names: Names<'_>, v: &Value) -> String {
    match v {
        Value::Uninit => "<uninitialized>".to_owned(),
        Value::Bool(b) => b.to_string(),
        Value::Int(i) => i.to_string(),
        Value::Decimal(d) => decimal(*d),
        Value::Rat(r) => format!("{}/{}", r.numer(), r.denom()),
        Value::String(s) => format!("{s:?}"),
        Value::Text(t) => format!("t![{:?}]", rich(names, t)),
        Value::Tuple(items) => {
            let inner = list(names, items);
            if items.len() == 1 {
                format!("({inner},)")
            } else {
                format!("({inner})")
            }
        }
        Value::Array(items) => format!("[{}]", list(names, items)),
        Value::Vec(items) => format!("vec![{}]", list(names, items)),
        Value::Adt(adt) => adt_display(names, adt),
        Value::Asset(id) => format!("asset({:?})", names.asset(*id)),
        Value::Handle(id) => format!("&{}", names.static_path(*id)),
        Value::Func(f) => names.function(*f).to_owned(),
        Value::Closure(c) => names.function(c.func).to_owned(),
        Value::Ref(_) => match deref(v) {
            Some(target) => display(names, &target),
            None => "<host part>".to_owned(),
        },
        Value::Host(h) => format!("<{}>", h.0.type_name()),
        Value::HostFn(_) => "<host fn>".to_owned(),
    }
}

fn decimal(d: Decimal) -> String {
    let digits = d.mantissa().unsigned_abs().to_string();
    let scale = d.scale() as usize;
    let sign = if d.mantissa() < 0 { "-" } else { "" };
    if scale == 0 {
        return format!("{sign}{digits}");
    }
    let padded = format!("{digits:0>width$}", width = scale.saturating_add(1));
    let split = padded.len().saturating_sub(scale);
    format!("{sign}{}.{}", &padded[..split], &padded[split..])
}

fn list(names: Names<'_>, items: &[Value]) -> String {
    items
        .iter()
        .map(|v| display(names, v))
        .collect::<Vec<_>>()
        .join(", ")
}

fn adt_display(names: Names<'_>, adt: &Adt) -> String {
    let path = names.types.path(adt.ty);
    let variant = names.types.variant_name(adt.ty, adt.variant);
    let head = if names.types.is_enum(adt.ty) {
        format!("{path}::{variant}")
    } else {
        path.to_owned()
    };
    if adt.fields.is_empty() {
        return head;
    }
    let fields = names.fields(adt.ty, adt.variant);
    let named = fields.first().is_some_and(Option::is_some);
    if named {
        let parts: Vec<String> = adt
            .fields
            .iter()
            .enumerate()
            .map(|(i, v)| {
                let name = fields.get(i).copied().flatten().unwrap_or("_");
                format!("{name}: {}", display(names, v))
            })
            .collect();
        format!("{head} {{ {} }}", parts.join(", "))
    } else {
        format!("{head}({})", list(names, &adt.fields))
    }
}

/// A named value with its parts as children (tuples, arrays, vectors and ADTs).
pub fn var(names: Names<'_>, name: impl Into<String>, v: &Value) -> Var {
    let target = deref(v);
    let shown = target.as_ref().unwrap_or(v);
    let children = match shown {
        Value::Tuple(items) | Value::Array(items) | Value::Vec(items) => items
            .iter()
            .enumerate()
            .map(|(i, item)| var(names, i.to_string(), item))
            .collect(),
        Value::Adt(adt) => {
            let fields = names.fields(adt.ty, adt.variant);
            adt.fields
                .iter()
                .enumerate()
                .map(|(i, item)| {
                    let name = fields
                        .get(i)
                        .copied()
                        .flatten()
                        .map_or_else(|| i.to_string(), str::to_owned);
                    var(names, name, item)
                })
                .collect()
        }
        _ => Vec::new(),
    };
    Var {
        name: name.into(),
        value: display(names, shown),
        children,
    }
}

/// A graph value as Kagari source would write it.
pub fn graph_display(v: &kd::Value) -> String {
    match v {
        kd::Value::Bool(b) => b.to_string(),
        kd::Value::Int(i) => i.to_string(),
        kd::Value::Decimal { mantissa, scale } => decimal(Decimal::new(*mantissa, *scale)),
        kd::Value::Rat { num, den } => format!("{num}/{den}"),
        kd::Value::String(s) => format!("{s:?}"),
        kd::Value::Text(t) => format!("t![{:?}]", graph_plain(t)),
        kd::Value::Tuple(items) => format!("({})", graph_list(items)),
        kd::Value::Array(items) => format!("[{}]", graph_list(items)),
        kd::Value::Vec(items) => format!("vec![{}]", graph_list(items)),
        kd::Value::Adt {
            ty,
            variant,
            fields,
        } => {
            let head = if ty.rsplit("::").next() == Some(variant.as_str()) {
                ty.clone()
            } else {
                format!("{ty}::{variant}")
            };
            if fields.is_empty() {
                head
            } else {
                format!("{head}({})", graph_list(fields))
            }
        }
        kd::Value::Asset(path) => format!("asset({path:?})"),
        kd::Value::Handle(path) => format!("&{path}"),
        kd::Value::Fn(path) | kd::Value::Closure(path) => path.clone(),
        kd::Value::Screen => "<Screen>".to_owned(),
    }
}

fn graph_list(items: &[kd::Value]) -> String {
    items
        .iter()
        .map(graph_display)
        .collect::<Vec<_>>()
        .join(", ")
}

fn graph_plain(t: &kd::Text) -> String {
    match t {
        kd::Text::Plain(s) => s.clone(),
        kd::Text::Ruby { base, .. }
        | kd::Text::MonoRuby { base, .. }
        | kd::Text::Effect { base, .. } => graph_plain(base),
        kd::Text::Concat(items) => items.iter().map(graph_plain).collect(),
    }
}
