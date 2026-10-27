//! Editing text inside one scene re-runs the queries of that scene only.

use std::path::PathBuf;
use std::sync::{Arc, Mutex, PoisonError};

use kagari_lang::def_map::def_maps;
use kagari_lang::item_tree::{ItemId, ItemIndex, item_tree};
use kagari_lang::mir::item_mir;
use kagari_lang::{FilePath, MemoryFileSystem, Program, RootDatabase, check};
use salsa::{Database, EventKind};

const MAIN: &str = "/game/main.kix";

const SOURCE: &str = r#"
fn main() {}

#[scene]
#[entry]
pushdown fn Main() {
    s!(t!["hello"]);
    Callee();
}

#[scene]
pushdown fn Callee() {
    s!(t!["bye"]);
}

const N: u32 = 3;
"#;

#[test]
fn editing_a_scene_reruns_only_its_queries() {
    let mut fs = MemoryFileSystem::default();
    fs.0.insert(PathBuf::from(MAIN), SOURCE.to_owned());
    let executed: Arc<Mutex<Vec<salsa::DatabaseKeyIndex>>> = Arc::default();
    let sink = executed.clone();
    let mut db = RootDatabase::with_event_callback(
        Arc::new(fs),
        Box::new(move |event| {
            if let EventKind::WillExecute { database_key } = event.kind {
                sink.lock()
                    .unwrap_or_else(PoisonError::into_inner)
                    .push(database_key);
            }
        }),
    );
    let program = db.program(FilePath::disk(MAIN));
    check(&db, program);
    lower_all(&db, program);

    executed
        .lock()
        .unwrap_or_else(PoisonError::into_inner)
        .clear();
    db.set_file_text(
        FilePath::disk(MAIN),
        Some(SOURCE.replace("\"hello\"", "\"hello there\"")),
    );
    check(&db, program);
    lower_all(&db, program);

    let keys = std::mem::take(&mut *executed.lock().unwrap_or_else(PoisonError::into_inner));
    let names: Vec<String> = keys
        .iter()
        .map(|key| query_name(&db.ingredient_debug_name(key.ingredient_index())))
        .collect();
    let count = |name: &str| names.iter().filter(|executed| *executed == name).count();
    assert_eq!(count("infer"), 1, "{names:?}");
    assert_eq!(count("item_mir"), 1, "{names:?}");
    assert_eq!(count("def_maps"), 0, "{names:?}");
    assert_eq!(count("program_signatures"), 0, "{names:?}");
    assert_eq!(count("const_value"), 0, "{names:?}");
}

/// Builds the MIR of every item, as codegen does.
fn lower_all(db: &RootDatabase, program: Program) {
    for (file, _) in &def_maps(db, program).files {
        for position in 0..item_tree(db, *file).items.len() {
            if let Some(index) = ItemIndex::from_usize(position) {
                let _ = item_mir(db, program, ItemId { file: *file, index });
            }
        }
    }
}

/// The query's function name: debug names may carry a suffix such as `::interned_arguments`.
fn query_name(debug_name: &str) -> String {
    debug_name
        .split("::")
        .next()
        .unwrap_or(debug_name)
        .to_owned()
}
