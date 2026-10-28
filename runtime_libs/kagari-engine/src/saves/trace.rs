//! The save file layout, and the ids a trace records for choices and arms.

use kagari_archive::save::{Save, Scalar, TableRef, ValueRef};
use kagari_vm::{FuncId, Program, Text, Value, Vm};

use super::Timestamp;
use crate::store::value::{Names, StoreError, decode, decode_text, encode_push, encode_text};
use crate::story::Trace;

/// The id a trace records for arm `arm` of choice `id` in `func`.
pub(crate) fn choice_id(program: &Program, func: FuncId, id: &str, arm: &str) -> String {
    format!("{}/{}/{}", program.function_path(func), id, arm)
}

/// The id of an arm: its value's variant path when it is an enum value, else its index.
pub(crate) fn arm_id(vm: &Vm, value: &Value, index: usize) -> String {
    let value = match value {
        Value::Ref(r) => vm.load(r).unwrap_or(Value::Uninit),
        other => other.clone(),
    };
    match &value {
        Value::Adt(adt) => format!(
            "{}::{}",
            vm.types().path(adt.ty),
            vm.types().variant_name(adt.ty, adt.variant)
        ),
        _ => format!("#{index}"),
    }
}

/// Everything a save file holds.
pub(crate) struct SaveFile {
    pub version: i64,
    pub script_hash: String,
    pub trace: Trace,
    pub on_exit: Option<String>,
    pub time: Timestamp,
    pub who: Option<Text>,
    pub text: Option<Text>,
    pub thumbnail: Option<Vec<u8>>,
}

/// The slot listing part of a save file.
pub(crate) struct SaveHead {
    pub time: Timestamp,
    pub who: Option<Text>,
    pub text: Option<Text>,
    pub thumbnail: Option<Vec<u8>>,
}

pub(crate) fn write(names: &Names, file: &SaveFile) -> Result<Save, StoreError> {
    let mut save = Save::new();
    let mut root = save.root_mut();
    root.set("version", Scalar::I64(file.version))?;
    root.set("script_hash", Scalar::String(&file.script_hash))?;
    root.set("entry", Scalar::String(&file.trace.entry))?;
    {
        let mut args = root.array("args")?;
        for a in &file.trace.args {
            encode_push(names, &mut args, a)?;
        }
    }
    root.set("seed", Scalar::U64(file.trace.seed))?;
    {
        let mut choices = root.array("choices")?;
        for c in &file.trace.choices {
            choices.push(Scalar::String(c))?;
        }
    }
    root.set(
        "steps_after_last_choice",
        Scalar::U64(u64::from(file.trace.steps_after_last_choice)),
    )?;
    if let Some(on_exit) = &file.on_exit {
        root.set("on_exit", Scalar::String(on_exit))?;
    }
    {
        let t = file.time;
        let mut time = root.table("time")?;
        for (name, v) in [
            ("year", i64::from(t.year)),
            ("month", i64::from(t.month)),
            ("day", i64::from(t.day)),
            ("hour", i64::from(t.hour)),
            ("minute", i64::from(t.minute)),
            ("second", i64::from(t.second)),
        ] {
            time.set(name, Scalar::I64(v))?;
        }
    }
    for (name, plain, text) in [
        ("who", "who_plain", &file.who),
        ("text", "text_plain", &file.text),
    ] {
        if let Some(text) = text {
            encode_text(names, &mut root, name, text)?;
            root.set(plain, Scalar::String(&crate::story::text_plain(text)))?;
        }
    }
    if let Some(png) = &file.thumbnail {
        root.set("thumbnail", Scalar::Bytes(png))?;
    }
    Ok(save)
}

fn missing(f: &str) -> String {
    format!("missing field `{f}`")
}

fn wrong(f: &str) -> String {
    format!("field `{f}` has the wrong type")
}

fn int(root: TableRef<'_>, f: &str) -> Result<i128, String> {
    match root.get(f) {
        None => Err(missing(f)),
        Some(ValueRef::I64(v)) => Ok(i128::from(v)),
        Some(ValueRef::U64(v)) => Ok(i128::from(v)),
        Some(ValueRef::I128(v)) => Ok(v),
        Some(_) => Err(wrong(f)),
    }
}

fn string<'a>(root: TableRef<'a>, f: &str) -> Result<&'a str, String> {
    match root.get(f) {
        None => Err(missing(f)),
        Some(ValueRef::String(s)) => Ok(s),
        Some(_) => Err(wrong(f)),
    }
}

fn narrow<T: TryFrom<i128>>(v: i128, f: &str) -> Result<T, String> {
    T::try_from(v).map_err(|_| wrong(f))
}

/// The version field, read first.
pub(crate) fn version(save: &Save) -> Result<i64, String> {
    narrow(int(save.root(), "version")?, "version")
}

pub(crate) fn parse_head(program: &Program, save: &Save) -> Result<SaveHead, String> {
    let root = save.root();
    let Some(ValueRef::Table(time)) = root.get("time") else {
        return Err(match root.get("time") {
            None => missing("time"),
            Some(_) => wrong("time"),
        });
    };
    let time = Timestamp {
        year: narrow(int(time, "year")?, "year")?,
        month: narrow(int(time, "month")?, "month")?,
        day: narrow(int(time, "day")?, "day")?,
        hour: narrow(int(time, "hour")?, "hour")?,
        minute: narrow(int(time, "minute")?, "minute")?,
        second: narrow(int(time, "second")?, "second")?,
    };
    let text = |f: &str| -> Result<Option<Text>, String> {
        match root.get(f) {
            None => Ok(None),
            Some(v) => decode_text(program, Some(v)).map(Some),
        }
    };
    let thumbnail = match root.get("thumbnail") {
        Some(ValueRef::Bytes(b)) => Some(b.to_vec()),
        _ => None,
    };
    Ok(SaveHead {
        time,
        who: text("who")?,
        text: text("text")?,
        thumbnail,
    })
}

/// The whole file. Arguments that no longer decode are `ParseError::Args`, since they are a
/// divergence, not an unreadable file.
pub(crate) fn parse(program: &Program, save: &Save) -> Result<SaveFile, ParseError> {
    let root = save.root();
    let head = parse_head(program, save).map_err(ParseError::Unreadable)?;
    let un = ParseError::Unreadable;
    let entry = string(root, "entry").map_err(un)?.to_owned();
    let args = match root.get("args") {
        Some(ValueRef::Array(a)) => a
            .iter()
            .map(|v| decode(program, v))
            .collect::<Result<Vec<_>, _>>()
            .map_err(|_| ParseError::Args(entry.clone()))?,
        None => return Err(un(missing("args"))),
        Some(_) => return Err(un(wrong("args"))),
    };
    let choices = match root.get("choices") {
        Some(ValueRef::Array(a)) => a
            .iter()
            .map(|v| match v {
                ValueRef::String(s) => Ok(s.to_owned()),
                _ => Err(wrong("choices")),
            })
            .collect::<Result<Vec<_>, _>>()
            .map_err(un)?,
        None => return Err(un(missing("choices"))),
        Some(_) => return Err(un(wrong("choices"))),
    };
    let seed = match root.get("seed") {
        Some(ValueRef::U64(s)) => s,
        Some(ValueRef::I64(s)) => s.cast_unsigned(),
        None => return Err(un(missing("seed"))),
        Some(_) => return Err(un(wrong("seed"))),
    };
    let steps = narrow(
        int(root, "steps_after_last_choice").map_err(un)?,
        "steps_after_last_choice",
    )
    .map_err(un)?;
    let on_exit = match root.get("on_exit") {
        Some(ValueRef::String(s)) => Some(s.to_owned()),
        _ => None,
    };
    Ok(SaveFile {
        version: version(save).map_err(un)?,
        script_hash: string(root, "script_hash").map_err(un)?.to_owned(),
        trace: Trace {
            entry,
            args,
            seed,
            choices,
            steps_after_last_choice: steps,
        },
        on_exit,
        time: head.time,
        who: head.who,
        text: head.text,
        thumbnail: head.thumbnail,
    })
}

pub(crate) enum ParseError {
    Unreadable(String),
    /// The arguments of this entry no longer decode.
    Args(String),
}
