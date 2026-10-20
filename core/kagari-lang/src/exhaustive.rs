//! Match exhaustiveness: a Maranget usefulness check over lazily expanded types.
//!
//! Shared by `.kis` `<match>` (stage 2) and `.kirp`/`.kix` `match` (stage 3).

/// The shape of a type, as far as patterns can observe it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum TyKind<T> {
    Bool,
    /// An inclusive integer domain.
    Int {
        min: i128,
        max: i128,
    },
    /// Payload types per variant.
    Enum(Vec<Vec<T>>),
    /// Field types, in declaration order.
    Struct(Vec<T>),
    /// Covered only by a wildcard (str, rational, unit, type parameters, errors).
    Opaque,
}

/// Expands types on demand, so recursive types terminate.
pub trait Types {
    type Ty: Clone;
    fn kind(&self, ty: &Self::Ty) -> TyKind<Self::Ty>;
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Pat {
    Wild,
    Bool(bool),
    /// An inclusive integer range; a literal is `Int(v, v)`.
    Int(i128, i128),
    Variant(usize, Vec<Pat>),
    /// All fields in declaration order; unmentioned fields are `Wild`.
    Struct(Vec<Pat>),
    Or(Vec<Pat>),
}

/// A value shape not covered by any row.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Witness {
    Wild,
    Bool(bool),
    Int(i128, i128),
    Variant(usize, Vec<Witness>),
    Struct(Vec<Witness>),
}

/// One value per column that no row matches, or `None` when the rows are exhaustive.
pub fn missing<T: Types>(types: &T, cols: &[T::Ty], rows: &[Vec<Pat>]) -> Option<Vec<Witness>> {
    let rows: Vec<Vec<&Pat>> = rows.iter().map(|row| row.iter().collect()).collect();
    search(types, cols, rows)
}

/// A head constructor of the first column.
#[derive(Clone, Copy, Debug)]
enum Ctor {
    Bool(bool),
    Int(i128, i128),
    Variant(usize),
    Struct,
}

fn search<T: Types>(types: &T, cols: &[T::Ty], rows: Vec<Vec<&Pat>>) -> Option<Vec<Witness>> {
    let Some((first, rest)) = cols.split_first() else {
        return if rows.is_empty() {
            Some(Vec::new())
        } else {
            None
        };
    };
    if rows.is_empty() {
        return Some(vec![Witness::Wild; cols.len()]);
    }
    let rows = expand_or(rows);
    let kind = types.kind(first);
    // With no constructor in the first column, only the wildcard rows matter and the
    // witness is a wildcard. Heads that do not fit the column's type (after an earlier
    // type error) act as wildcards, except that an opaque column is covered only by `_`.
    let is_wild = |pat: &Pat| match &kind {
        TyKind::Opaque => matches!(pat, Pat::Wild),
        _ => !fits(&kind, pat),
    };
    if matches!(kind, TyKind::Opaque)
        || rows
            .iter()
            .all(|row| row.first().is_none_or(|pat| is_wild(pat)))
    {
        let default: Vec<Vec<&Pat>> = rows
            .iter()
            .filter(|row| row.first().is_some_and(|pat| is_wild(pat)))
            .map(|row| row.iter().skip(1).copied().collect())
            .collect();
        let mut witness = search(types, rest, default)?;
        witness.insert(0, Witness::Wild);
        return Some(witness);
    }
    let ctors: Vec<(Ctor, Vec<T::Ty>)> = match &kind {
        TyKind::Bool => vec![
            (Ctor::Bool(false), Vec::new()),
            (Ctor::Bool(true), Vec::new()),
        ],
        TyKind::Int { min, max } => split_ints(*min, *max, &rows)
            .into_iter()
            .map(|(lo, hi)| (Ctor::Int(lo, hi), Vec::new()))
            .collect(),
        TyKind::Enum(variants) => variants
            .iter()
            .enumerate()
            .map(|(index, payload)| (Ctor::Variant(index), payload.clone()))
            .collect(),
        TyKind::Struct(fields) => vec![(Ctor::Struct, fields.clone())],
        TyKind::Opaque => Vec::new(),
    };
    for (ctor, sub_tys) in ctors {
        let arity = sub_tys.len();
        let specialized: Vec<Vec<&Pat>> = rows
            .iter()
            .filter_map(|row| specialize(&kind, ctor, arity, row))
            .collect();
        let mut tys = sub_tys;
        tys.extend(rest.iter().cloned());
        if let Some(mut witness) = search(types, &tys, specialized) {
            let tail = witness.split_off(arity.min(witness.len()));
            let head = match ctor {
                Ctor::Bool(value) => Witness::Bool(value),
                Ctor::Int(lo, hi) => Witness::Int(lo, hi),
                Ctor::Variant(index) => Witness::Variant(index, witness),
                Ctor::Struct => Witness::Struct(witness),
            };
            let mut out = Vec::with_capacity(cols.len());
            out.push(head);
            out.extend(tail);
            return Some(out);
        }
    }
    None
}

/// Replace every row whose head is an or-pattern by one row per alternative.
fn expand_or(rows: Vec<Vec<&Pat>>) -> Vec<Vec<&Pat>> {
    let mut out = Vec::with_capacity(rows.len());
    let mut stack: Vec<Vec<&Pat>> = rows.into_iter().rev().collect();
    while let Some(row) = stack.pop() {
        match row.first() {
            Some(Pat::Or(alternatives)) => {
                for alternative in alternatives.iter().rev() {
                    let mut new_row = Vec::with_capacity(row.len());
                    new_row.push(alternative);
                    new_row.extend(row.iter().skip(1).copied());
                    stack.push(new_row);
                }
            }
            _ => out.push(row),
        }
    }
    out
}

/// Split `[min, max]` at every range boundary in the first column into disjoint intervals,
/// so each pattern either contains an interval or is disjoint from it.
fn split_ints(min: i128, max: i128, rows: &[Vec<&Pat>]) -> Vec<(i128, i128)> {
    if min > max {
        return Vec::new();
    }
    let end = max.saturating_add(1);
    let mut points = vec![min, end];
    for row in rows {
        if let Some(Pat::Int(lo, hi)) = row.first() {
            for point in [*lo, hi.saturating_add(1)] {
                if point > min && point < end {
                    points.push(point);
                }
            }
        }
    }
    points.sort_unstable();
    points.dedup();
    points
        .windows(2)
        .filter_map(|pair| match pair {
            [lo, next] => Some((*lo, next.saturating_sub(1))),
            _ => None,
        })
        .collect()
}

/// The row specialized by `ctor`, or `None` when the head cannot match it.
/// A head that does not fit the column's type (after an earlier type error) acts as a wildcard.
fn specialize<'p>(
    kind: &TyKind<impl Clone>,
    ctor: Ctor,
    arity: usize,
    row: &[&'p Pat],
) -> Option<Vec<&'p Pat>> {
    const WILD: &Pat = &Pat::Wild;
    let (head, tail) = row.split_first()?;
    let mut out: Vec<&'p Pat> = Vec::with_capacity(arity.saturating_add(tail.len()));
    if !fits(kind, head) {
        out.extend(std::iter::repeat_n(WILD, arity));
    } else {
        match (ctor, head) {
            (Ctor::Bool(want), Pat::Bool(have)) if want != *have => return None,
            (Ctor::Int(lo, hi), Pat::Int(plo, phi)) if !(*plo <= lo && hi <= *phi) => return None,
            (Ctor::Variant(want), Pat::Variant(have, _)) if want != *have => return None,
            (_, Pat::Variant(_, subs) | Pat::Struct(subs)) => {
                out.extend(subs.iter().take(arity));
                let len = subs.len().min(arity);
                out.extend(std::iter::repeat_n(WILD, arity.saturating_sub(len)));
            }
            _ => {}
        }
    }
    out.extend(tail.iter().copied());
    Some(out)
}

/// Whether `pat` is a constructor pattern of the column type `kind`.
fn fits<T>(kind: &TyKind<T>, pat: &Pat) -> bool {
    matches!(
        (kind, pat),
        (TyKind::Bool, Pat::Bool(_))
            | (TyKind::Int { .. }, Pat::Int(..))
            | (TyKind::Enum(_), Pat::Variant(..))
            | (TyKind::Struct(_), Pat::Struct(_))
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    /// `0` = bool, `1` = int 0..=5, `2` = enum { A, B(bool), C }, `3` = list (recursive), `4` = opaque.
    struct Toy;

    impl Types for Toy {
        type Ty = u8;
        fn kind(&self, ty: &u8) -> TyKind<u8> {
            match ty {
                0 => TyKind::Bool,
                1 => TyKind::Int { min: 0, max: 5 },
                2 => TyKind::Enum(vec![vec![], vec![0], vec![]]),
                3 => TyKind::Enum(vec![vec![], vec![1, 3]]),
                _ => TyKind::Opaque,
            }
        }
    }

    #[test]
    fn bool_missing_false() {
        let rows = vec![vec![Pat::Bool(true)]];
        assert_eq!(missing(&Toy, &[0], &rows), Some(vec![Witness::Bool(false)]));
    }

    #[test]
    fn int_hole() {
        let rows = vec![vec![Pat::Int(0, 2)], vec![Pat::Int(4, 5)]];
        assert_eq!(missing(&Toy, &[1], &rows), Some(vec![Witness::Int(3, 3)]));
        let rows = vec![vec![Pat::Int(0, 2)], vec![Pat::Int(3, 5)]];
        assert_eq!(missing(&Toy, &[1], &rows), None);
    }

    #[test]
    fn two_columns() {
        let rows = vec![
            vec![Pat::Bool(true), Pat::Wild],
            vec![Pat::Bool(false), Pat::Bool(true)],
        ];
        assert_eq!(
            missing(&Toy, &[0, 0], &rows),
            Some(vec![Witness::Bool(false), Witness::Bool(false)])
        );
    }

    #[test]
    fn enum_payload_and_or() {
        let rows = vec![vec![Pat::Or(vec![
            Pat::Variant(0, vec![]),
            Pat::Variant(1, vec![Pat::Bool(true)]),
        ])]];
        assert_eq!(
            missing(&Toy, &[2], &rows),
            Some(vec![Witness::Variant(1, vec![Witness::Bool(false)])])
        );
        let rows = vec![
            vec![Pat::Variant(0, vec![])],
            vec![Pat::Variant(1, vec![Pat::Wild])],
            vec![Pat::Variant(2, vec![])],
        ];
        assert_eq!(missing(&Toy, &[2], &rows), None);
    }

    #[test]
    fn recursive_and_opaque() {
        let rows = vec![
            vec![Pat::Variant(0, vec![])],
            vec![Pat::Variant(1, vec![Pat::Wild, Pat::Variant(0, vec![])])],
        ];
        assert_eq!(
            missing(&Toy, &[3], &rows),
            Some(vec![Witness::Variant(
                1,
                vec![
                    Witness::Wild,
                    Witness::Variant(1, vec![Witness::Wild, Witness::Wild])
                ]
            )])
        );
        assert_eq!(
            missing(&Toy, &[4], &[vec![Pat::Int(1, 1)]]),
            Some(vec![Witness::Wild])
        );
        assert_eq!(missing(&Toy, &[4], &[vec![Pat::Wild]]), None);
        assert_eq!(missing(&Toy, &[4], &[]), Some(vec![Witness::Wild]));
    }
}
