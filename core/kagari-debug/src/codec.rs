//! Typed access to `.kisave` fields: ids and counts are `I64` cells, optional fields are
//! absent or `Null`.

use kagari_archive::save::{ArrayMut, ArrayRef, Scalar, TableMut, TableRef, ValueRef};

use crate::Error;

pub(crate) fn field<'a>(t: TableRef<'a>, name: &'static str) -> Result<ValueRef<'a>, Error> {
    t.get(name).ok_or(Error::Malformed(name))
}

pub(crate) fn int(v: ValueRef<'_>) -> Option<i128> {
    match v {
        ValueRef::I64(v) => Some(i128::from(v)),
        ValueRef::U64(v) => Some(i128::from(v)),
        ValueRef::I128(v) => Some(v),
        _ => None,
    }
}

pub(crate) fn as_u32(v: ValueRef<'_>, name: &'static str) -> Result<u32, Error> {
    int(v)
        .and_then(|v| u32::try_from(v).ok())
        .ok_or(Error::Malformed(name))
}

pub(crate) fn u32_field(t: TableRef<'_>, name: &'static str) -> Result<u32, Error> {
    as_u32(field(t, name)?, name)
}

pub(crate) fn opt_u32_field(t: TableRef<'_>, name: &'static str) -> Result<Option<u32>, Error> {
    match t.get(name) {
        None | Some(ValueRef::Null) => Ok(None),
        Some(v) => as_u32(v, name).map(Some),
    }
}

pub(crate) fn i64_field(t: TableRef<'_>, name: &'static str) -> Result<i64, Error> {
    match field(t, name)? {
        ValueRef::I64(v) => Ok(v),
        _ => Err(Error::Malformed(name)),
    }
}

pub(crate) fn bool_field(t: TableRef<'_>, name: &'static str) -> Result<bool, Error> {
    match field(t, name)? {
        ValueRef::Bool(v) => Ok(v),
        _ => Err(Error::Malformed(name)),
    }
}

pub(crate) fn as_str<'a>(v: ValueRef<'a>, name: &'static str) -> Result<&'a str, Error> {
    match v {
        ValueRef::String(s) => Ok(s),
        _ => Err(Error::Malformed(name)),
    }
}

pub(crate) fn str_field<'a>(t: TableRef<'a>, name: &'static str) -> Result<&'a str, Error> {
    as_str(field(t, name)?, name)
}

pub(crate) fn as_table<'a>(v: ValueRef<'a>, name: &'static str) -> Result<TableRef<'a>, Error> {
    match v {
        ValueRef::Table(t) => Ok(t),
        _ => Err(Error::Malformed(name)),
    }
}

pub(crate) fn table_field<'a>(t: TableRef<'a>, name: &'static str) -> Result<TableRef<'a>, Error> {
    as_table(field(t, name)?, name)
}

pub(crate) fn as_array<'a>(v: ValueRef<'a>, name: &'static str) -> Result<ArrayRef<'a>, Error> {
    match v {
        ValueRef::Array(a) => Ok(a),
        _ => Err(Error::Malformed(name)),
    }
}

pub(crate) fn array_field<'a>(t: TableRef<'a>, name: &'static str) -> Result<ArrayRef<'a>, Error> {
    as_array(field(t, name)?, name)
}

/// Every element of the array field `name`, read with `read`.
pub(crate) fn list<T>(
    t: TableRef<'_>,
    name: &'static str,
    read: impl FnMut(ValueRef<'_>) -> Result<T, Error>,
) -> Result<Vec<T>, Error> {
    array_field(t, name)?.iter().map(read).collect()
}

/// The array field `name` of ids.
pub(crate) fn ids(t: TableRef<'_>, name: &'static str) -> Result<Vec<u32>, Error> {
    list(t, name, |v| as_u32(v, name))
}

/// The array field `name` of strings.
pub(crate) fn strings(t: TableRef<'_>, name: &'static str) -> Result<Vec<String>, Error> {
    list(t, name, |v| as_str(v, name).map(str::to_owned))
}

pub(crate) fn id(v: u32) -> Scalar<'static> {
    Scalar::I64(i64::from(v))
}

pub(crate) fn opt_id(v: Option<u32>) -> Scalar<'static> {
    v.map_or(Scalar::Null, id)
}

/// Sets the array field `name` to `values`.
pub(crate) fn put_ids(t: &mut TableMut<'_>, name: &str, values: &[u32]) -> Result<(), Error> {
    let mut a = t.array(name)?;
    for v in values {
        a.push(id(*v))?;
    }
    Ok(())
}

pub(crate) fn put_strings(
    t: &mut TableMut<'_>,
    name: &str,
    values: &[String],
) -> Result<(), Error> {
    let mut a = t.array(name)?;
    push_strings(&mut a, values)
}

pub(crate) fn push_strings(a: &mut ArrayMut<'_>, values: &[String]) -> Result<(), Error> {
    for v in values {
        a.push(Scalar::String(v))?;
    }
    Ok(())
}
