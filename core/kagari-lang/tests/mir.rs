//! The MIR of every game item with a body, for each bytecode fixture.

use std::sync::Arc;

use kagari_lang::def_map::{CrateId, def_maps};
use kagari_lang::item_tree::{ItemId, ItemIndex, item_tree};
use kagari_lang::mir::pretty::dump_item;
use kagari_lang::{FilePath, OsFileSystem, RootDatabase, check};

#[test]
fn mir() {
    insta::glob!("bytecode/*/main.k{is,ix}", |path| {
        let db = RootDatabase::new(Arc::new(OsFileSystem));
        let program = db.program(FilePath::disk(path));
        check(&db, program);
        let maps = def_maps(&db, program);
        let files: Vec<_> = maps
            .files
            .iter()
            .filter(|(_, krate)| *krate == CrateId::Game)
            .map(|(file, _)| *file)
            .collect();
        let mut out = String::new();
        for file in files {
            let tree = item_tree(&db, file);
            for position in 0..tree.items.len() {
                let Some(index) = ItemIndex::from_usize(position) else {
                    continue;
                };
                let dump = dump_item(&db, program, ItemId { file, index });
                if !dump.is_empty() {
                    if !out.is_empty() {
                        out.push('\n');
                    }
                    out.push_str(&dump);
                }
            }
        }
        insta::assert_snapshot!(out);
    });
}
