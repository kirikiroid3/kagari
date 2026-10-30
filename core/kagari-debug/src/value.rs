//! Story values, stored the way the engine stores values in saves: scalars as scalars,
//! everything else as a table tagged by the field `"$"`, with types, variants, assets and
//! statics named by path. Functions, closures and screens, which saves cannot hold, get the
//! tags `fn`, `closure` and `screen`.

use kagari_archive::save::{ArrayMut, Scalar, TableMut, TableRef, ValueRef};

use crate::Error;
use crate::codec::{array_field, as_array, field, str_field};

/// A value of the story: a kept local, a story variable, an argument or a drawn value.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub enum Value {
    Bool(bool),
    Int(i128),
    /// `mantissa / 10^scale`.
    Decimal {
        mantissa: i128,
        scale: u32,
    },
    Rat {
        num: i128,
        den: i128,
    },
    String(String),
    Text(Text),
    Tuple(Vec<Value>),
    Array(Vec<Value>),
    Vec(Vec<Value>),
    /// A struct or enum value: the type's path, the variant name (a struct's single variant
    /// is named like the struct) and the fields in declaration order.
    Adt {
        ty: String,
        variant: String,
        fields: Vec<Value>,
    },
    /// An asset by its path in the bytecode.
    Asset(String),
    /// A `Setting<T>` or `Meta<T>` handle by the path of its static.
    Handle(String),
    /// A named function by its instance path.
    Fn(String),
    /// A closure that captures nothing, by its instance path (`…::{closure#N}`).
    Closure(String),
    /// A screen; its content is not stored.
    Screen,
}

/// A text: plain, or rich as built by `t![…]`.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub enum Text {
    Plain(String),
    Ruby {
        base: Box<Text>,
        reading: Box<Text>,
    },
    MonoRuby {
        base: Box<Text>,
        readings: Vec<Text>,
    },
    Effect {
        base: Box<Text>,
        effect: Box<Value>,
    },
    Concat(Vec<Text>),
}

impl Value {
    pub fn unit() -> Value {
        Value::Tuple(Vec::new())
    }

    fn scalar(&self) -> Option<Scalar<'_>> {
        Some(match self {
            Value::Bool(b) => Scalar::Bool(*b),
            Value::Int(i) => Scalar::I128(*i),
            Value::Decimal { mantissa, scale } => Scalar::Decimal {
                mantissa: *mantissa,
                scale: *scale,
            },
            Value::Rat { num, den } => Scalar::Rat {
                num: *num,
                den: *den,
            },
            Value::String(s) => Scalar::String(s),
            _ => return None,
        })
    }

    /// Appends the value to `a`.
    pub(crate) fn push(&self, a: &mut ArrayMut<'_>) -> Result<(), Error> {
        if let Some(scalar) = self.scalar() {
            return Ok(a.push(scalar)?);
        }
        let mut t = a.push_table()?;
        self.fill(&mut t)
    }

    /// Writes the value as the field `name` of `t`.
    pub(crate) fn put(&self, t: &mut TableMut<'_>, name: &str) -> Result<(), Error> {
        if let Some(scalar) = self.scalar() {
            return Ok(t.set(name, scalar)?);
        }
        let mut child = t.table(name)?;
        self.fill(&mut child)
    }

    fn fill(&self, t: &mut TableMut<'_>) -> Result<(), Error> {
        let items = |t: &mut TableMut<'_>, tag: &str, items: &[Value]| -> Result<(), Error> {
            t.set("$", Scalar::String(tag))?;
            let mut a = t.array("items")?;
            items.iter().try_for_each(|item| item.push(&mut a))
        };
        let path = |t: &mut TableMut<'_>, tag: &str, path: &str| -> Result<(), Error> {
            t.set("$", Scalar::String(tag))?;
            Ok(t.set("path", Scalar::String(path))?)
        };
        match self {
            Value::Text(text) => {
                t.set("$", Scalar::String("text"))?;
                text.put(t, "t")
            }
            Value::Tuple(xs) => items(t, "tuple", xs),
            Value::Array(xs) => items(t, "array", xs),
            Value::Vec(xs) => items(t, "vec", xs),
            Value::Adt {
                ty,
                variant,
                fields,
            } => {
                t.set("$", Scalar::String("adt"))?;
                t.set("type", Scalar::String(ty))?;
                t.set("variant", Scalar::String(variant))?;
                let mut a = t.array("fields")?;
                fields.iter().try_for_each(|field| field.push(&mut a))
            }
            Value::Asset(p) => path(t, "asset", p),
            Value::Handle(p) => path(t, "handle", p),
            Value::Fn(p) => path(t, "fn", p),
            Value::Closure(p) => path(t, "closure", p),
            Value::Screen => Ok(t.set("$", Scalar::String("screen"))?),
            // Scalars are written by the callers.
            Value::Bool(_)
            | Value::Int(_)
            | Value::Decimal { .. }
            | Value::Rat { .. }
            | Value::String(_) => Ok(()),
        }
    }

    pub(crate) fn read(v: ValueRef<'_>) -> Result<Value, Error> {
        Ok(match v {
            ValueRef::Bool(b) => Value::Bool(b),
            ValueRef::I64(i) => Value::Int(i128::from(i)),
            ValueRef::U64(u) => Value::Int(i128::from(u)),
            ValueRef::I128(i) => Value::Int(i),
            ValueRef::Decimal { mantissa, scale } => Value::Decimal { mantissa, scale },
            ValueRef::Rat { num, den } => Value::Rat { num, den },
            ValueRef::String(s) => Value::String(s.to_owned()),
            ValueRef::Table(t) => Value::read_table(t)?,
            _ => return Err(Error::Malformed("value")),
        })
    }

    fn read_table(t: TableRef<'_>) -> Result<Value, Error> {
        let items = |t: TableRef<'_>| -> Result<Vec<Value>, Error> {
            array_field(t, "items")?.iter().map(Value::read).collect()
        };
        let path = |t: TableRef<'_>| str_field(t, "path").map(str::to_owned);
        Ok(match str_field(t, "$")? {
            "text" => Value::Text(Text::read(field(t, "t")?)?),
            "tuple" => Value::Tuple(items(t)?),
            "array" => Value::Array(items(t)?),
            "vec" => Value::Vec(items(t)?),
            "adt" => Value::Adt {
                ty: str_field(t, "type")?.to_owned(),
                variant: str_field(t, "variant")?.to_owned(),
                fields: array_field(t, "fields")?
                    .iter()
                    .map(Value::read)
                    .collect::<Result<_, _>>()?,
            },
            "asset" => Value::Asset(path(t)?),
            "handle" => Value::Handle(path(t)?),
            "fn" => Value::Fn(path(t)?),
            "closure" => Value::Closure(path(t)?),
            "screen" => Value::Screen,
            _ => return Err(Error::Malformed("$")),
        })
    }
}

impl Text {
    /// Writes the text as the field `name` of `t` (a plain text as a string).
    fn put(&self, t: &mut TableMut<'_>, name: &str) -> Result<(), Error> {
        if let Text::Plain(s) = self {
            return Ok(t.set(name, Scalar::String(s))?);
        }
        let mut node = t.table(name)?;
        self.fill(&mut node)
    }

    fn push(&self, a: &mut ArrayMut<'_>) -> Result<(), Error> {
        if let Text::Plain(s) = self {
            return Ok(a.push(Scalar::String(s))?);
        }
        let mut node = a.push_table()?;
        self.fill(&mut node)
    }

    fn fill(&self, node: &mut TableMut<'_>) -> Result<(), Error> {
        match self {
            // Plain texts are written as strings by the callers.
            Text::Plain(_) => Ok(()),
            Text::Ruby { base, reading } => {
                node.set("$", Scalar::String("ruby"))?;
                base.put(node, "base")?;
                reading.put(node, "reading")
            }
            Text::MonoRuby { base, readings } => {
                node.set("$", Scalar::String("mono_ruby"))?;
                base.put(node, "base")?;
                let mut a = node.array("readings")?;
                readings.iter().try_for_each(|reading| reading.push(&mut a))
            }
            Text::Effect { base, effect } => {
                node.set("$", Scalar::String("effect"))?;
                base.put(node, "base")?;
                effect.put(node, "effect")
            }
            Text::Concat(items) => {
                node.set("$", Scalar::String("concat"))?;
                let mut a = node.array("items")?;
                items.iter().try_for_each(|item| item.push(&mut a))
            }
        }
    }

    fn read(v: ValueRef<'_>) -> Result<Text, Error> {
        let t = match v {
            ValueRef::String(s) => return Ok(Text::Plain(s.to_owned())),
            ValueRef::Table(t) => t,
            _ => return Err(Error::Malformed("text")),
        };
        let texts = |name: &'static str| -> Result<Vec<Text>, Error> {
            as_array(field(t, name)?, name)?
                .iter()
                .map(Text::read)
                .collect()
        };
        let text = |name: &'static str| -> Result<Box<Text>, Error> {
            Ok(Box::new(Text::read(field(t, name)?)?))
        };
        Ok(match str_field(t, "$")? {
            "ruby" => Text::Ruby {
                base: text("base")?,
                reading: text("reading")?,
            },
            "mono_ruby" => Text::MonoRuby {
                base: text("base")?,
                readings: texts("readings")?,
            },
            "effect" => Text::Effect {
                base: text("base")?,
                effect: Box::new(Value::read(field(t, "effect")?)?),
            },
            "concat" => Text::Concat(texts("items")?),
            _ => return Err(Error::Malformed("$")),
        })
    }
}
