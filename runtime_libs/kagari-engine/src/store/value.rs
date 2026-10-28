//! Storing VM values in `.kisave` files: scalars as scalars, everything else as tables tagged by
//! the field `"$"`.

use std::rc::Rc;

use kagari_archive::save::{
    ArrayMut, ArrayRef, Error as SaveError, Scalar, TableMut, TableRef, ValueRef,
};
use kagari_vm::{Adt, AdtTy, AssetId, Decimal, Program, Rat, StaticId, Text, TypeTable, Value};

/// Why a value could not be written.
#[derive(Debug)]
pub(crate) enum StoreError {
    /// The value holds something with this tag, which no file can hold.
    NotStorable(&'static str),
    Save(SaveError),
}

impl std::fmt::Display for StoreError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            StoreError::NotStorable(tag) => write!(f, "it holds a {tag}"),
            StoreError::Save(e) => write!(f, "{e}"),
        }
    }
}

impl From<SaveError> for StoreError {
    fn from(e: SaveError) -> StoreError {
        StoreError::Save(e)
    }
}

/// The names a stored value refers to: type paths (module and synthetic), asset and static paths.
pub(crate) struct Names<'a> {
    pub program: &'a Program,
    pub types: &'a TypeTable,
}

impl Names<'_> {
    fn asset(&self, id: AssetId) -> &str {
        let module = self.program.module();
        module
            .assets
            .get(id.0 as usize)
            .map_or("", |a| self.program.string(a.path))
    }

    fn static_path(&self, id: StaticId) -> &str {
        let module = self.program.module();
        module
            .statics
            .get(id.0 as usize)
            .map_or("", |s| self.program.string(s.path))
    }
}

/// `Ok` when `v` can be stored; else the tag of what cannot.
pub(crate) fn storable(v: &Value) -> Result<(), &'static str> {
    match v {
        Value::Bool(_)
        | Value::Int(_)
        | Value::Decimal(_)
        | Value::Rat(_)
        | Value::String(_)
        | Value::Asset(_)
        | Value::Handle(_) => Ok(()),
        Value::Text(t) => text_storable(t),
        Value::Tuple(items) | Value::Array(items) | Value::Vec(items) => {
            items.iter().try_for_each(storable)
        }
        Value::Adt(adt) => adt.fields.iter().try_for_each(storable),
        other => Err(other.tag()),
    }
}

fn text_storable(t: &Text) -> Result<(), &'static str> {
    match t {
        Text::Plain(_) => Ok(()),
        Text::Ruby(p) => text_storable(&p.0).and(text_storable(&p.1)),
        Text::MonoRuby(p) => {
            text_storable(&p.0)?;
            p.1.iter().try_for_each(text_storable)
        }
        Text::Effect(p) => text_storable(&p.0).and(storable(&p.1)),
        Text::Concat(items) => items.iter().try_for_each(text_storable),
    }
}

/// The trap message of a value a `what` (`Meta`, `Setting`) cannot hold, or `None`.
pub(crate) fn check_storable(v: &Value, what: &str) -> Result<(), kagari_vm::Trap> {
    storable(v).map_err(|tag| {
        kagari_vm::Trap::host(format!(
            "a `{what}` value cannot be stored: it holds a {tag}"
        ))
    })
}

/// Writes `v` as the field `name` of `t`.
pub(crate) fn encode_field(
    names: &Names,
    t: &mut TableMut<'_>,
    name: &str,
    v: &Value,
) -> Result<(), StoreError> {
    if let Some(s) = scalar(v)? {
        return Ok(t.set(name, s)?);
    }
    let mut table = t.table(name)?;
    fill(names, &mut table, v)
}

/// Appends `v` to `a`.
pub(crate) fn encode_push(
    names: &Names,
    a: &mut ArrayMut<'_>,
    v: &Value,
) -> Result<(), StoreError> {
    if let Some(s) = scalar(v)? {
        return Ok(a.push(s)?);
    }
    let mut table = a.push_table()?;
    fill(names, &mut table, v)
}

fn scalar(v: &Value) -> Result<Option<Scalar<'_>>, StoreError> {
    Ok(Some(match v {
        Value::Bool(b) => Scalar::Bool(*b),
        Value::Int(i) => Scalar::I128(*i),
        Value::Decimal(d) => Scalar::Decimal {
            mantissa: d.mantissa(),
            scale: d.scale(),
        },
        Value::Rat(r) => Scalar::Rat {
            num: r.numer(),
            den: r.denom(),
        },
        Value::String(s) => Scalar::String(s),
        Value::Text(_)
        | Value::Tuple(_)
        | Value::Array(_)
        | Value::Vec(_)
        | Value::Adt(_)
        | Value::Asset(_)
        | Value::Handle(_) => return Ok(None),
        other => return Err(StoreError::NotStorable(other.tag())),
    }))
}

fn items(
    names: &Names,
    t: &mut TableMut<'_>,
    tag: &str,
    items: &[Value],
) -> Result<(), StoreError> {
    t.set("$", Scalar::String(tag))?;
    let mut a = t.array("items")?;
    for item in items {
        encode_push(names, &mut a, item)?;
    }
    Ok(())
}

/// Fills an empty table with a non-scalar value.
fn fill(names: &Names, t: &mut TableMut<'_>, v: &Value) -> Result<(), StoreError> {
    match v {
        Value::Text(text) => {
            t.set("$", Scalar::String("text"))?;
            encode_text(names, t, "t", text)
        }
        Value::Tuple(xs) => items(names, t, "tuple", xs),
        Value::Array(xs) => items(names, t, "array", xs),
        Value::Vec(xs) => items(names, t, "vec", xs),
        Value::Adt(adt) => {
            t.set("$", Scalar::String("adt"))?;
            t.set("type", Scalar::String(names.types.path(adt.ty)))?;
            t.set(
                "variant",
                Scalar::String(names.types.variant_name(adt.ty, adt.variant)),
            )?;
            let mut a = t.array("fields")?;
            for f in &adt.fields {
                encode_push(names, &mut a, f)?;
            }
            Ok(())
        }
        Value::Asset(id) => {
            t.set("$", Scalar::String("asset"))?;
            Ok(t.set("path", Scalar::String(names.asset(*id)))?)
        }
        Value::Handle(id) => {
            t.set("$", Scalar::String("handle"))?;
            Ok(t.set("path", Scalar::String(names.static_path(*id)))?)
        }
        other => Err(StoreError::NotStorable(other.tag())),
    }
}

/// Writes a text node as the field `name` of `t` (a plain text as a string).
pub(crate) fn encode_text(
    names: &Names,
    t: &mut TableMut<'_>,
    name: &str,
    text: &Text,
) -> Result<(), StoreError> {
    if let Text::Plain(s) = text {
        return Ok(t.set(name, Scalar::String(s))?);
    }
    let mut node = t.table(name)?;
    write_text_node(names, &mut node, text)
}

fn push_text(names: &Names, a: &mut ArrayMut<'_>, text: &Text) -> Result<(), StoreError> {
    if let Text::Plain(s) = text {
        return Ok(a.push(Scalar::String(s))?);
    }
    let mut holder = a.push_table()?;
    // Only non-plain nodes reach here.
    write_text_node(names, &mut holder, text)
}

fn write_text_node(names: &Names, node: &mut TableMut<'_>, text: &Text) -> Result<(), StoreError> {
    match text {
        // Plain texts are written as strings by the callers.
        Text::Plain(_) => Ok(()),
        Text::Ruby(p) => {
            node.set("$", Scalar::String("ruby"))?;
            encode_text(names, node, "base", &p.0)?;
            encode_text(names, node, "reading", &p.1)
        }
        Text::MonoRuby(p) => {
            node.set("$", Scalar::String("mono_ruby"))?;
            encode_text(names, node, "base", &p.0)?;
            let mut a = node.array("readings")?;
            for r in &p.1 {
                push_text(names, &mut a, r)?;
            }
            Ok(())
        }
        Text::Effect(p) => {
            node.set("$", Scalar::String("effect"))?;
            encode_text(names, node, "base", &p.0)?;
            encode_field(names, node, "effect", &p.1)
        }
        Text::Concat(xs) => {
            node.set("$", Scalar::String("concat"))?;
            let mut a = node.array("items")?;
            for x in xs.iter() {
                push_text(names, &mut a, x)?;
            }
            Ok(())
        }
    }
}

/// Reads a stored value back.
pub(crate) fn decode(program: &Program, v: ValueRef<'_>) -> Result<Value, String> {
    Ok(match v {
        ValueRef::Bool(b) => Value::Bool(b),
        ValueRef::I64(i) => Value::Int(i128::from(i)),
        ValueRef::U64(u) => Value::Int(i128::from(u)),
        ValueRef::I128(i) => Value::Int(i),
        ValueRef::Decimal { mantissa, scale } => Value::Decimal(Decimal::new(mantissa, scale)),
        ValueRef::Rat { num, den } => {
            Value::Rat(Rat::new(num, den).ok_or_else(|| "an invalid `Rat`".to_owned())?)
        }
        ValueRef::String(s) => Value::String(Rc::from(s)),
        ValueRef::Table(t) => decode_table(program, t)?,
        other => return Err(format!("unexpected stored value {other:?}")),
    })
}

fn str_field<'a>(t: TableRef<'a>, name: &str) -> Result<&'a str, String> {
    match t.get(name) {
        Some(ValueRef::String(s)) => Ok(s),
        _ => Err(format!("missing field `{name}`")),
    }
}

fn array_field<'a>(t: TableRef<'a>, name: &str) -> Result<ArrayRef<'a>, String> {
    match t.get(name) {
        Some(ValueRef::Array(a)) => Ok(a),
        _ => Err(format!("missing field `{name}`")),
    }
}

fn decode_items(program: &Program, t: TableRef<'_>) -> Result<Rc<Vec<Value>>, String> {
    let items = array_field(t, "items")?
        .iter()
        .map(|v| decode(program, v))
        .collect::<Result<Vec<_>, _>>()?;
    Ok(Rc::new(items))
}

fn decode_table(program: &Program, t: TableRef<'_>) -> Result<Value, String> {
    let module = program.module();
    Ok(match str_field(t, "$")? {
        "text" => Value::Text(decode_text(program, t.get("t"))?),
        "tuple" => Value::Tuple(decode_items(program, t)?),
        "array" => Value::Array(decode_items(program, t)?),
        "vec" => Value::Vec(decode_items(program, t)?),
        "adt" => {
            let path = str_field(t, "type")?;
            let ty = program
                .find_type(path)
                .ok_or_else(|| format!("unknown type `{path}`"))?;
            let decl = module
                .types
                .get(ty.0 as usize)
                .ok_or_else(|| format!("unknown type `{path}`"))?;
            let name = str_field(t, "variant")?;
            let (index, variant) = decl
                .variants
                .iter()
                .enumerate()
                .find(|(_, v)| program.string(v.name) == name)
                .ok_or_else(|| format!("unknown variant `{path}::{name}`"))?;
            let fields = array_field(t, "fields")?
                .iter()
                .map(|v| decode(program, v))
                .collect::<Result<Vec<_>, _>>()?;
            if fields.len() != variant.fields.len() {
                return Err(format!("`{path}::{name}` has a different number of fields"));
            }
            Value::Adt(Rc::new(Adt {
                ty: AdtTy::Module(ty),
                variant: u32::try_from(index).map_err(|_| "too many variants".to_owned())?,
                fields,
            }))
        }
        "asset" => {
            let path = str_field(t, "path")?;
            let index = module
                .assets
                .iter()
                .position(|a| program.string(a.path) == path)
                .ok_or_else(|| format!("unknown asset `{path}`"))?;
            Value::Asset(AssetId(
                u32::try_from(index).map_err(|_| "too many assets".to_owned())?,
            ))
        }
        "handle" => {
            let path = str_field(t, "path")?;
            let index = module
                .statics
                .iter()
                .position(|s| program.string(s.path) == path)
                .ok_or_else(|| format!("unknown static `{path}`"))?;
            Value::Handle(StaticId(
                u32::try_from(index).map_err(|_| "too many statics".to_owned())?,
            ))
        }
        other => return Err(format!("unknown stored tag `{other}`")),
    })
}

/// A stored text node.
pub(crate) fn decode_text(program: &Program, v: Option<ValueRef<'_>>) -> Result<Text, String> {
    let t = match v {
        Some(ValueRef::String(s)) => return Ok(Text::Plain(Rc::from(s))),
        Some(ValueRef::Table(t)) => t,
        _ => return Err("a malformed text".to_owned()),
    };
    let texts = |name: &str| -> Result<Vec<Text>, String> {
        array_field(t, name)?
            .iter()
            .map(|v| decode_text(program, Some(v)))
            .collect()
    };
    Ok(match str_field(t, "$")? {
        "ruby" => Text::Ruby(Rc::new((
            decode_text(program, t.get("base"))?,
            decode_text(program, t.get("reading"))?,
        ))),
        "mono_ruby" => Text::MonoRuby(Rc::new((
            decode_text(program, t.get("base"))?,
            texts("readings")?,
        ))),
        "effect" => Text::Effect(Rc::new((
            decode_text(program, t.get("base"))?,
            decode(
                program,
                t.get("effect")
                    .ok_or_else(|| "missing field `effect`".to_owned())?,
            )?,
        ))),
        "concat" => Text::Concat(Rc::new(texts("items")?)),
        other => return Err(format!("unknown text node `{other}`")),
    })
}

/// Whether a stored value still has the shape of the initial value.
pub(crate) fn same_shape(stored: &Value, init: &Value) -> bool {
    match (stored, init) {
        (Value::Adt(a), Value::Adt(b)) => {
            a.ty == b.ty
                && (a.variant != b.variant
                    || a.fields
                        .iter()
                        .zip(&b.fields)
                        .all(|(x, y)| same_shape(x, y)))
        }
        (Value::Tuple(a), Value::Tuple(b)) => {
            a.len() == b.len() && a.iter().zip(b.iter()).all(|(x, y)| same_shape(x, y))
        }
        (Value::Vec(a), Value::Vec(b)) | (Value::Array(a), Value::Array(b)) => match b.first() {
            Some(first) => a.iter().all(|x| same_shape(x, first)),
            None => true,
        },
        _ => stored.tag() == init.tag(),
    }
}
