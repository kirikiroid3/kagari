#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
#![allow(clippy::arithmetic_side_effects)]

use super::*;

fn u64_at(b: &[u8], off: usize) -> u64 {
    u64::from_le_bytes(b[off..off + 8].try_into().unwrap())
}

fn put_u64(b: &mut [u8], off: usize, v: u64) {
    b[off..off + 8].copy_from_slice(&v.to_le_bytes());
}

fn reload(save: &Save) -> Save {
    Save::from_bytes(save.as_bytes().to_vec()).unwrap()
}

/// A deep, position-independent rendering of a value.
fn dump(v: ValueRef<'_>) -> String {
    match v {
        ValueRef::Table(t) => {
            let fields: Vec<_> = t.iter().map(|(k, v)| format!("{k}: {}", dump(v))).collect();
            format!("{{{}}}", fields.join(", "))
        }
        ValueRef::Array(a) => {
            let items: Vec<_> = a.iter().map(dump).collect();
            format!("[{}]", items.join(", "))
        }
        other => format!("{other:?}"),
    }
}

fn dump_root(save: &Save) -> String {
    dump(ValueRef::Table(save.root()))
}

/// Root field 0's slot is at 64 when the root has not moved; its cell's type is at 80 and its
/// value at 82.
const SLOT0: usize = 64;
const TYPE0: usize = 80;
const VALUE0: usize = 82;

#[test]
fn fresh_save() {
    let save = Save::new();
    let b = save.as_bytes();
    assert_eq!(b.len(), 16384);
    assert_eq!(b[0..16], *b"SAVEKAGARI\0\0V000");
    assert_eq!(u64_at(b, 16), 48);
    assert_eq!(u64_at(b, 24), 3392);
    assert_eq!(u64_at(b, 48), 0);
    assert_eq!(u64_at(b, 56), 128);
    assert_eq!(save.used_bytes(), 3392);
    let save = Save::from_bytes(Save::new().into_bytes()).unwrap();
    assert!(save.root().is_empty());
    assert_eq!(save.root().cap(), ROOT_CAP);
}

#[test]
fn scalars_round_trip() {
    let values = [
        ("bool", Scalar::Bool(true), ValueRef::Bool(true)),
        (
            "bytes",
            Scalar::Bytes(&[0, 255, 7]),
            ValueRef::Bytes(&[0, 255, 7]),
        ),
        (
            "dec",
            Scalar::Decimal {
                mantissa: 5,
                scale: 1,
            },
            ValueRef::Decimal {
                mantissa: 5,
                scale: 1,
            },
        ),
        ("empty", Scalar::String(""), ValueRef::String("")),
        ("i128", Scalar::I128(i128::MIN), ValueRef::I128(i128::MIN)),
        ("i64", Scalar::I64(-42), ValueRef::I64(-42)),
        ("null", Scalar::Null, ValueRef::Null),
        (
            "rat",
            Scalar::Rat { num: -1, den: 2 },
            ValueRef::Rat { num: -1, den: 2 },
        ),
        (
            "str",
            Scalar::String("こんにちは, kagari"),
            ValueRef::String("こんにちは, kagari"),
        ),
        ("u128", Scalar::U128(u128::MAX), ValueRef::U128(u128::MAX)),
        ("u64", Scalar::U64(u64::MAX), ValueRef::U64(u64::MAX)),
    ];
    let mut save = Save::new();
    let mut root = save.root_mut();
    for (name, value, _) in values {
        root.set(name, value).unwrap();
    }
    let save = Save::from_bytes(save.into_bytes()).unwrap();
    let root = save.root();
    assert_eq!(root.len(), values.len() as u64);
    for (name, _, expected) in values {
        assert_eq!(root.get(name), Some(expected), "{name}");
    }
    let names: Vec<_> = root.iter().map(|(k, _)| k).collect();
    let expected: Vec<_> = values.iter().map(|v| v.0).collect();
    assert_eq!(names, expected);
}

#[test]
fn inline_and_fixed_overwrites_do_not_allocate() {
    let mut save = Save::new();
    let before = save.used_bytes();
    save.root_mut().set("count", Scalar::I64(1)).unwrap();
    assert_eq!(save.used_bytes(), before + "count".len() as u64);

    let before = save.used_bytes();
    save.root_mut().set("count", Scalar::I64(2)).unwrap();
    save.root_mut().set("count", Scalar::Bool(false)).unwrap();
    assert_eq!(save.used_bytes(), before);

    save.root_mut().set("big", Scalar::I128(1)).unwrap();
    let before = save.used_bytes();
    save.root_mut().set("big", Scalar::I128(-7)).unwrap();
    assert_eq!(save.used_bytes(), before);
    assert_eq!(reload(&save).root().get("big"), Some(ValueRef::I128(-7)));
}

#[test]
fn root_growth() {
    let mut save = Save::new();
    let mut root = save.root_mut();
    // Insert out of order so every insert shifts slots.
    for i in (0..200).rev() {
        root.set(&format!("k{i:03}"), Scalar::U64(i)).unwrap();
    }
    assert_ne!(u64_at(save.as_bytes(), 16), 48);
    let expected: Vec<_> = (0..200).map(|i| format!("k{i:03}")).collect();
    let names: Vec<_> = save.root().iter().map(|(k, _)| k.to_owned()).collect();
    assert_eq!(names, expected);

    let loaded = reload(&save);
    assert_eq!(loaded.root().cap(), 256);
    for i in 0..200 {
        assert_eq!(
            loaded.root().get(&format!("k{i:03}")),
            Some(ValueRef::U64(i))
        );
    }
}

#[test]
fn nested_growth() {
    let mut save = Save::new();
    let mut root = save.root_mut();
    let mut t = root.table("t").unwrap();
    for i in 0..20 {
        t.set(&format!("f{i:02}"), Scalar::I64(i)).unwrap();
    }
    let mut v = root.array("v").unwrap();
    for i in 0..20 {
        v.push(Scalar::String(&format!("item {i}"))).unwrap();
    }
    v.push_table()
        .unwrap()
        .set("inner", Scalar::Bool(true))
        .unwrap();
    v.push_array().unwrap().push(Scalar::Null).unwrap();

    let save = reload(&save);
    let Some(ValueRef::Table(t)) = save.root().get("t") else {
        panic!()
    };
    assert_eq!((t.len(), t.cap()), (20, 32));
    for i in 0..20 {
        assert_eq!(t.get(&format!("f{i:02}")), Some(ValueRef::I64(i)));
    }
    let Some(ValueRef::Array(v)) = save.root().get("v") else {
        panic!()
    };
    assert_eq!((v.len(), v.cap()), (22, 32));
    assert_eq!(v.get(19), Some(ValueRef::String("item 19")));
    assert_eq!(v.get(22), None);
    assert_eq!(dump(v.get(20).unwrap()), "{inner: Bool(true)}");
    assert_eq!(dump(v.get(21).unwrap()), "[Null]");
}

#[test]
fn array_edits() {
    let mut save = Save::new();
    let mut root = save.root_mut();
    let mut a = root.array("a").unwrap();
    for i in 0..5 {
        a.push(Scalar::U64(i)).unwrap();
    }
    a.set(1, Scalar::String("one")).unwrap();
    assert_eq!(a.set(5, Scalar::Null), Err(Error::IndexOutOfRange));
    a.push_table().unwrap();
    assert_eq!(a.table_at(0).err(), Some(Error::TypeMismatch));
    assert_eq!(a.array_at(9).err(), Some(Error::IndexOutOfRange));
    a.table_at(5).unwrap().set("x", Scalar::Null).unwrap();
    a.truncate(9);
    assert_eq!(a.len(), 6);
    a.truncate(2);
    assert_eq!(a.len(), 2);

    assert_eq!(dump_root(&reload(&save)), r#"{a: [U64(0), String("one")]}"#);
}

#[test]
fn paging() {
    let mut save = Save::new();
    let long = "x".repeat(40_000);
    save.root_mut().set("long", Scalar::String(&long)).unwrap();
    let len = save.as_bytes().len() as u64;
    assert_eq!(len % 16384, 0);
    assert!(len >= 3392 + 40_016);
    assert_eq!(
        reload(&save).root().get("long"),
        Some(ValueRef::String(&long))
    );
}

#[test]
fn remove_overwrite_compact() {
    let mut save = Save::new();
    let mut root = save.root_mut();
    root.set("a", Scalar::U64(1)).unwrap();
    for i in 0..10 {
        root.set("name", Scalar::String(&format!("version {i}")))
            .unwrap();
    }
    let mut gone = root.table("gone").unwrap();
    gone.set("x", Scalar::Rat { num: 1, den: 3 }).unwrap();
    let mut keep = root.table("keep").unwrap();
    keep.set(
        "d",
        Scalar::Decimal {
            mantissa: 125,
            scale: 2,
        },
    )
    .unwrap();
    keep.array("list")
        .unwrap()
        .push(Scalar::Bytes(b"ab"))
        .unwrap();
    for i in 0..200 {
        root.set(&format!("z{i:03}"), Scalar::Null).unwrap();
    }
    assert!(root.remove("gone"));
    assert!(!root.remove("gone"));
    assert_eq!(root.get("gone"), None);
    assert_ne!(u64_at(save.as_bytes(), 16), 48);

    let compact = save.compact();
    assert!(compact.used_bytes() < save.used_bytes());
    assert_eq!(u64_at(compact.as_bytes(), 16), 48);
    assert_eq!(compact.root().cap(), save.root().cap());
    assert_eq!(dump_root(&compact), dump_root(&save));
    assert_eq!(dump_root(&reload(&compact)), dump_root(&save));
    assert!(dump_root(&compact).starts_with(
        r#"{a: U64(1), keep: {d: Decimal { mantissa: 125, scale: 2 }, list: [Bytes([97, 98])]}, name: String("version 9"), z000: Null"#
    ));
}

/// Descends `n` more tables, then checks that the next level is refused.
fn nest(mut t: TableMut<'_>, n: u32) {
    if n > 0 {
        return nest(t.table("t").unwrap(), n - 1);
    }
    assert_eq!(t.table("t").err(), Some(Error::TooDeep));
    assert_eq!(t.array("a").err(), Some(Error::TooDeep));
    t.set("leaf", Scalar::U64(1)).unwrap();
}

#[test]
fn depth_limit() {
    let mut save = Save::new();
    nest(save.root_mut(), MAX_DEPTH);
    let loaded = reload(&save);
    assert_eq!(dump_root(&loaded.compact()), dump_root(&save));
}

#[test]
fn type_mismatch_and_names() {
    let mut save = Save::new();
    let mut root = save.root_mut();
    root.set("x", Scalar::I64(1)).unwrap();
    assert_eq!(root.table("x").err(), Some(Error::TypeMismatch));
    assert_eq!(root.array("x").err(), Some(Error::TypeMismatch));
    assert_eq!(root.set("", Scalar::Null), Err(Error::BadName));
    assert_eq!(root.table("").err(), Some(Error::BadName));
}

/// A save whose root field 0 is `name` set to `value`, as bytes.
fn one_field(name: &str, value: Scalar<'_>) -> Vec<u8> {
    let mut save = Save::new();
    save.root_mut().set(name, value).unwrap();
    save.into_bytes()
}

#[test]
fn validation() {
    let good = one_field("x", Scalar::U64(0));
    assert!(Save::from_bytes(good.clone()).is_ok());

    assert_eq!(
        Save::from_bytes(good[..16383].to_vec()),
        Err(Error::BadSize)
    );
    assert_eq!(Save::from_bytes(Vec::new()), Err(Error::BadSize));

    let mut b = good.clone();
    b[0] = b'X';
    assert_eq!(Save::from_bytes(b), Err(Error::BadMagic));

    let mut b = good.clone();
    b[13..16].copy_from_slice(b"001");
    assert_eq!(Save::from_bytes(b), Err(Error::UnsupportedVersion));

    let mut b = good.clone();
    put_u64(&mut b, 40, 1);
    assert_eq!(Save::from_bytes(b), Err(Error::NonZeroReserved));

    // A table field pointing past `end`.
    let mut b = one_field("t", Scalar::Null);
    b[TYPE0..TYPE0 + 2].copy_from_slice(&ValueType::Table.id().to_le_bytes());
    put_u64(&mut b, VALUE0, 10_000);
    assert_eq!(Save::from_bytes(b), Err(Error::OutOfBounds));

    // Swapped names.
    let mut save = Save::new();
    save.root_mut().set("a", Scalar::Null).unwrap();
    save.root_mut().set("b", Scalar::Null).unwrap();
    let mut b = save.into_bytes();
    let (s0, s1) = (SLOT0, SLOT0 + 26);
    let name0: [u8; 16] = b[s0..s0 + 16].try_into().unwrap();
    b.copy_within(s1..s1 + 16, s0);
    b[s1..s1 + 16].copy_from_slice(&name0);
    assert_eq!(Save::from_bytes(b), Err(Error::NotSorted));

    // A field pointing back at the root.
    let mut b = good.clone();
    b[TYPE0..TYPE0 + 2].copy_from_slice(&ValueType::Table.id().to_le_bytes());
    put_u64(&mut b, VALUE0, 48);
    assert_eq!(Save::from_bytes(b), Err(Error::SharedNode));

    // Two strings sharing one head.
    let mut save = Save::new();
    save.root_mut().set("a", Scalar::String("shared")).unwrap();
    save.root_mut().set("b", Scalar::String("other")).unwrap();
    let mut b = save.into_bytes();
    let head = u64_at(&b, VALUE0);
    put_u64(&mut b, VALUE0 + 26, head);
    assert_eq!(Save::from_bytes(b), Err(Error::SharedNode));

    let mut b = one_field("s", Scalar::String("abc"));
    let data = u64_at(&b, u64_at(&b, VALUE0) as usize) as usize;
    b[data] = 0xFF;
    assert_eq!(Save::from_bytes(b), Err(Error::BadUtf8));

    let mut b = good.clone();
    b[TYPE0..TYPE0 + 2].copy_from_slice(&999u16.to_le_bytes());
    assert_eq!(Save::from_bytes(b), Err(Error::UnknownType(999)));

    let mut b = one_field("b", Scalar::Bool(true));
    put_u64(&mut b, VALUE0, 2);
    assert_eq!(Save::from_bytes(b), Err(Error::BadValue));

    let mut b = good.clone();
    put_u64(&mut b, SLOT0 + 8, 0);
    assert_eq!(Save::from_bytes(b), Err(Error::BadName));
}
