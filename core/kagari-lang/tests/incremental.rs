use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};

use kagari_lang::{FilePath, FileSystem, RootDatabase, check};
use rustc_hash::FxHashMap;

#[derive(Default)]
struct MemoryFs {
    files: Mutex<FxHashMap<PathBuf, String>>,
}

impl MemoryFs {
    fn insert(&self, path: &str, text: &str) {
        self.files
            .lock()
            .unwrap()
            .insert(PathBuf::from(path), text.to_owned());
    }
}

impl FileSystem for MemoryFs {
    fn read(&self, path: &Path) -> Option<String> {
        self.files.lock().unwrap().get(path).cloned()
    }
}

fn main_kis(scenes: &str) -> String {
    format!(
        r#"<kagari>
  <system start="title"/>
  <game>
    <import src="a.kix"/>
    <declare-enum name="Ending"><enum name="Normal" repr="0"/></declare-enum>
    <scene name="start" entry="true">
      <s>Hello.</s>
      <call scene="next"/>
      <return literal="Normal"/>
    </scene>
{scenes}  </game>
</kagari>
"#
    )
}

fn codes(db: &RootDatabase, program: kagari_lang::Program) -> Vec<&'static str> {
    check(db, program)
        .iter()
        .map(|diag| diag.code.as_str())
        .collect()
}

#[test]
fn edits_are_picked_up() {
    let fs = Arc::new(MemoryFs::default());
    fs.insert("/g/main.kis", &main_kis(""));
    fs.insert("/g/a.kix", "fn f() -> u64 { 1 }\n");
    let mut db = RootDatabase::new(fs);
    let program = db.program(FilePath::disk("/g/main.kis"));

    assert_eq!(codes(&db, program), ["kis/unknown-scene"]);

    let fixed = main_kis("    <scene name=\"next\">\n      <s>Bye.</s>\n    </scene>\n");
    db.set_file_text(FilePath::disk("/g/main.kis"), Some(fixed));
    assert_eq!(codes(&db, program), Vec::<&str>::new());

    db.set_file_text(
        FilePath::disk("/g/a.kix"),
        Some("fn f() -> u64 { true }\n".to_owned()),
    );
    assert_eq!(codes(&db, program), ["type/mismatch"]);
}
