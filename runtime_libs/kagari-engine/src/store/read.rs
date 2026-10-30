//! Read flags: per scene (a function; each instance of a generic scene is its own function),
//! the step IDs of the lines the player has read, as segments.
//!
//! A scene's read steps are a `Vec<usize>` `[a0, a1, a2, a3, …]`: each pair `[a2n, a2n+1]`
//! is the inclusive range of read step ids `a2n..=a2n+1`. The pairs are ascending, and two
//! pairs never overlap or touch (`a2n+2 > a2n+1 + 1`), so a scene read straight through is
//! one pair. `meta.kisave` stores them as `read.{function path}`, an array of `u64`.

use std::collections::{BTreeMap, HashMap};

use kagari_archive::save::{Scalar, TableMut, TableRef, ValueRef};
use kagari_vm::{FuncId, Program, StepId};

/// The read step ids of one scene, as inclusive ranges `[start, end, start, end, …]`.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub(crate) struct Segments(Vec<usize>);

impl Segments {
    fn pairs(&self) -> &[[usize; 2]] {
        self.0.as_chunks::<2>().0
    }

    /// The number of pairs whose start is at most `id`.
    fn starts_at_most(&self, id: usize) -> usize {
        self.pairs().partition_point(|&[start, _]| start <= id)
    }

    pub fn contains(&self, id: usize) -> bool {
        let at = self.starts_at_most(id);
        at.checked_sub(1)
            .and_then(|i| self.pairs().get(i))
            .is_some_and(|&[_, end]| id <= end)
    }

    /// Adds `id`, merging it with the pairs it touches; whether it was new.
    pub fn insert(&mut self, id: usize) -> bool {
        let at = self.starts_at_most(id);
        let before = at.checked_sub(1).and_then(|i| self.pairs().get(i).copied());
        if before.is_some_and(|[_, end]| id <= end) {
            return false;
        }
        let after = self.pairs().get(at).copied();
        let joins_before = before.is_some_and(|[_, end]| end.checked_add(1) == Some(id));
        let joins_after = after.is_some_and(|[start, _]| id.checked_add(1) == Some(start));
        // Pair `at` starts at `self.0[2 × at]`; pair `at - 1` ends just before it.
        let next = at.saturating_mul(2).min(self.0.len());
        let prev_end = next.checked_sub(1);
        match (joins_before, joins_after, prev_end) {
            // `[s, id - 1] [id + 1, e]` becomes `[s, e]`.
            (true, true, Some(prev_end)) => {
                self.0.drain(prev_end..=next);
            }
            (true, false, Some(prev_end)) => {
                if let Some(end) = self.0.get_mut(prev_end) {
                    *end = id;
                }
            }
            (false, true, _) => {
                if let Some(start) = self.0.get_mut(next) {
                    *start = id;
                }
            }
            _ => {
                self.0.splice(next..next, [id, id]);
            }
        }
        true
    }

    /// The segments of a stored array; tolerant of anything malformed: pairs are sorted,
    /// overlapping and touching pairs merged, reversed pairs and a trailing odd value dropped.
    fn from_stored(values: impl Iterator<Item = usize>) -> Segments {
        let values: Vec<usize> = values.collect();
        let mut pairs: Vec<[usize; 2]> = values
            .as_chunks::<2>()
            .0
            .iter()
            .copied()
            .filter(|&[start, end]| start <= end)
            .collect();
        pairs.sort_unstable();
        let mut out: Vec<[usize; 2]> = Vec::with_capacity(pairs.len());
        for [start, end] in pairs {
            match out.last_mut() {
                Some(last) if start <= last[1].saturating_add(1) => last[1] = last[1].max(end),
                _ => out.push([start, end]),
            }
        }
        Segments(out.into_iter().flatten().collect())
    }

    pub fn as_slice(&self) -> &[usize] {
        &self.0
    }
}

/// Every scene's read steps: by function for this program, by path for the scenes it lacks
/// (kept so that a patch that brings them back finds them).
#[derive(Default)]
pub(crate) struct ReadSet {
    scenes: HashMap<FuncId, Segments>,
    retained: BTreeMap<String, Segments>,
}

impl ReadSet {
    pub fn contains(&self, step: StepId) -> bool {
        let Ok(id) = usize::try_from(step.id) else {
            return false;
        };
        self.scenes
            .get(&step.func)
            .is_some_and(|segments| segments.contains(id))
    }

    /// Marks `step` read; whether it was new.
    pub fn insert(&mut self, step: StepId) -> bool {
        let Ok(id) = usize::try_from(step.id) else {
            return false;
        };
        self.scenes.entry(step.func).or_default().insert(id)
    }

    /// The `read` table of `meta.kisave`. Entries that are not arrays (the read keys of
    /// earlier engines, `{path}#{ordinal}` → `true`) are dropped.
    pub fn load(program: &Program, table: TableRef<'_>) -> ReadSet {
        let mut set = ReadSet::default();
        for (path, value) in table.iter() {
            let ValueRef::Array(values) = value else {
                continue;
            };
            let segments = Segments::from_stored(values.iter().filter_map(|v| match v {
                ValueRef::U64(n) => usize::try_from(n).ok(),
                _ => None,
            }));
            if segments.0.is_empty() {
                continue;
            }
            match program.find_function(path) {
                Some(func) => {
                    set.scenes.insert(func, segments);
                }
                None => {
                    set.retained.insert(path.to_owned(), segments);
                }
            }
        }
        set
    }

    /// Writes every scene's segments into `table`, by function path.
    pub fn save(&self, program: &Program, table: &mut TableMut<'_>) -> Result<(), String> {
        let mut scenes: BTreeMap<&str, &Segments> = self
            .retained
            .iter()
            .map(|(path, segments)| (path.as_str(), segments))
            .collect();
        for (func, segments) in &self.scenes {
            scenes.insert(program.function_path(*func), segments);
        }
        for (path, segments) in scenes {
            let mut array = table.array(path).map_err(|e| e.to_string())?;
            for &id in segments.as_slice() {
                let id = u64::try_from(id).map_err(|e| e.to_string())?;
                array.push(Scalar::U64(id)).map_err(|e| e.to_string())?;
            }
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used, clippy::arithmetic_side_effects)]

    use super::Segments;

    fn build(ids: &[usize]) -> Segments {
        let mut segments = Segments::default();
        for &id in ids {
            segments.insert(id);
        }
        segments
    }

    #[test]
    fn inserts_merge_into_canonical_segments() {
        // Apart, then touching on the left, on the right, and on both sides.
        assert_eq!(build(&[7, 3]).as_slice(), [3, 3, 7, 7]);
        assert_eq!(build(&[7, 3, 4]).as_slice(), [3, 4, 7, 7]);
        assert_eq!(build(&[7, 3, 6]).as_slice(), [3, 3, 6, 7]);
        assert_eq!(build(&[7, 3, 4, 6, 5]).as_slice(), [3, 7]);
        assert_eq!(build(&[0, 1, 2, 9, 8, 10]).as_slice(), [0, 2, 8, 10]);
        assert_eq!(
            build(&[usize::MAX, 0]).as_slice(),
            [0, 0, usize::MAX, usize::MAX]
        );
        assert_eq!(
            build(&[usize::MAX, usize::MAX - 1]).as_slice(),
            [usize::MAX - 1, usize::MAX]
        );
    }

    #[test]
    fn insert_reports_new_ids_and_contains_respects_bounds() {
        let mut segments = build(&[2, 3, 4, 8]);
        assert!(!segments.insert(3));
        assert!(!segments.insert(8));
        for (id, read) in [
            (1, false),
            (2, true),
            (4, true),
            (5, false),
            (8, true),
            (9, false),
        ] {
            assert_eq!(segments.contains(id), read, "{id}");
        }
        assert!(segments.insert(5));
        assert_eq!(segments.as_slice(), [2, 5, 8, 8]);
        assert!(!Segments::default().contains(0));
    }

    #[test]
    fn stored_segments_are_normalized() {
        let load = |values: &[usize]| Segments::from_stored(values.iter().copied());
        assert_eq!(load(&[0, 2, 5, 9]).as_slice(), [0, 2, 5, 9]);
        // Unsorted, overlapping, touching, reversed, and a trailing odd value.
        assert_eq!(load(&[5, 9, 0, 2, 3, 4, 8, 6, 20]).as_slice(), [0, 9]);
        assert_eq!(load(&[4, 6, 1, 5]).as_slice(), [1, 6]);
        assert_eq!(load(&[3, 1]).as_slice(), [] as [usize; 0]);
    }
}
