use std::path::{Path, PathBuf};
use std::sync::Arc;

use kagari_lang::ir::{Domain, IrReturn, IrScene, Op, SceneId, VarId, narrative_ir};
use kagari_lang::{FilePath, FileSystem, RootDatabase, check};

struct OneFile(String);

impl FileSystem for OneFile {
    fn read(&self, path: &Path) -> Option<String> {
        (path == Path::new("/g/main.kis")).then(|| self.0.clone())
    }
}

const PROGRAM: &str = r#"<kagari>
  <system start="title"/>
  <game>
    <declare-enum name="Ending">
      <enum name="A" repr="0"/>
      <enum name="B" repr="5"/>
    </declare-enum>
    <var-enum name="g" type="Ending" init="A"/>
    <scene name="side"><s>Side.</s></scene>
    <scene name="main" entry="true">
      <var-enum name="e" type="Ending" init="A"/>
      <call scene="pick" assign="e"/>
      <call scene="side"/>
      <return var="e"/>
    </scene>
    <scene name="pick" returns="Ending"><return literal="B"/></scene>
    <scene name="count" returns="char"><return literal="7"/></scene>
    <scene name="after" entry="true"><return literal="A"/></scene>
  </game>
</kagari>
"#;

fn ops(scene: &IrScene) -> Vec<&Op> {
    scene.nodes.iter().map(|node| &node.op).collect()
}

fn returns(scene: &IrScene) -> Vec<Op> {
    ops(scene)
        .into_iter()
        .filter(|op| matches!(op, Op::Return { .. }))
        .cloned()
        .collect()
}

#[test]
fn return_values_lower_to_ir() {
    let db = RootDatabase::new(Arc::new(OneFile(PROGRAM.to_owned())));
    let program = db.program(FilePath::disk(PathBuf::from("/g/main.kis")));
    assert!(check(&db, program).is_empty());
    let ir = narrative_ir(&db, program);

    // Scenes in declaration order: side, main, pick, count, after.
    assert_eq!(ir.entries, [SceneId(1), SceneId(4)]);
    let [side, main, pick, count, after] = ir.scenes.as_slice() else {
        panic!("expected five scenes");
    };
    assert_eq!(side.returns, None);
    assert_eq!(main.returns, Some(Domain::Enum { size: 2 }));
    assert_eq!(count.returns, Some(Domain::Range { min: 0, max: 255 }));

    // Globals are numbered before locals: `g` is 0, `main`'s `e` is 1.
    assert_eq!(
        returns(main),
        [Op::Return {
            value: Some(IrReturn::Var(VarId(1))),
            next: main.exit
        }]
    );
    let calls: Vec<(SceneId, Option<VarId>)> = ops(main)
        .into_iter()
        .filter_map(|op| match op {
            Op::Call { scene, assign, .. } => Some((*scene, *assign)),
            _ => None,
        })
        .collect();
    assert!(calls.contains(&(SceneId(2), Some(VarId(1)))));
    assert!(calls.contains(&(SceneId(0), None)));

    // Literals are domain indices: variant `B` is index 1 (not its repr 5); char 7 is 7.
    let literal = |scene: &IrScene, index: u32| {
        [Op::Return {
            value: Some(IrReturn::Literal(index)),
            next: scene.exit,
        }]
    };
    assert_eq!(returns(pick), literal(pick, 1));
    assert_eq!(returns(count), literal(count, 7));
    assert_eq!(returns(after), literal(after, 0));
}
