#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

//! Debug builds, the analysis cache shared by `check` and `build`, and `kagari clean`.

mod common;

use std::path::Path;

use common::{game, init_project, kagari, stderr, stdout};
use kagari_lang::kagari_debug::{
    AnalysisCache, DebugArtifact, DiagnosticRecord, Severity, SourceRef,
};
use sha2::{Digest, Sha256};

fn files(dir: &Path) -> Vec<String> {
    let mut files: Vec<String> = std::fs::read_dir(dir)
        .unwrap()
        .map(|entry| entry.unwrap().file_name().into_string().unwrap())
        .collect();
    files.sort();
    files
}

#[test]
fn a_debug_build_writes_the_story_graph_and_the_same_bytecode() {
    let tmp = init_project();
    let dir = game(&tmp);
    let output = kagari(&["build", "--profile", "release"], &dir);
    assert_eq!(output.status.code(), Some(0), "{}", stderr(&output));
    let out = dir.join("target/v0");
    let release = std::fs::read(out.join("game.kagari")).unwrap();

    let output = kagari(&["build", "--profile", "debug"], &dir);
    assert_eq!(output.status.code(), Some(0), "{}", stderr(&output));
    assert!(
        stdout(&output).contains("wrote target/v0/game.kagari-cache"),
        "{}",
        stdout(&output)
    );
    let bytes = std::fs::read(out.join("game.kagari")).unwrap();
    assert_eq!(
        bytes, release,
        "the bytecode does not depend on the profile"
    );

    let artifact =
        DebugArtifact::from_bytes(std::fs::read(out.join("game.kagari-cache")).unwrap()).unwrap();
    let hash: String = Sha256::digest(&bytes)
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect();
    assert_eq!(artifact.script_hash, hash);
    let module = kagari_bytecode::decode(&bytes).unwrap();
    assert_eq!(artifact.debug.functions.len(), module.functions.len());
    for (function, debug) in module.functions.iter().zip(&artifact.debug.functions) {
        assert_eq!(module.strings[function.path.0 as usize], debug.path);
    }

    // Every reached step is a step of the bytecode.
    let story = artifact.story.unwrap();
    assert!(story.complete);
    let disasm = stdout(&kagari(&["disasm", "target/v0/game.kagari"], &dir));
    let steps = story.reached_steps();
    assert!(!steps.is_empty());
    for step in &steps {
        assert!(disasm.contains(step.as_str()), "{step} is not in\n{disasm}");
    }

    // A release build removes the artifact of an earlier debug build.
    let output = kagari(&["build"], &dir);
    assert_eq!(output.status.code(), Some(0), "{}", stderr(&output));
    assert!(!out.join("game.kagari-cache").exists());
}

#[test]
fn check_and_build_reuse_the_analysis_until_a_source_changes() {
    let tmp = init_project();
    let dir = game(&tmp);
    let output = kagari(&["check"], &dir);
    assert_eq!(output.status.code(), Some(0), "{}", stdout(&output));
    let path = dir.join("target/analysis.kagari-cache");
    let mut cache = AnalysisCache::from_bytes(std::fs::read(&path).unwrap()).unwrap();
    assert!(cache.story.is_some());

    // A diagnostic only the cache holds shows that the analysis was not run again.
    let main = dir.join("src/main.kis");
    cache.diagnostics.push(DiagnosticRecord {
        file: SourceRef::Disk(main.to_str().unwrap().to_owned()),
        start: 0,
        end: 0,
        severity: Severity::Warning,
        code: "W5009".to_owned(),
        message: "only in the cache".to_owned(),
        related: Vec::new(),
    });
    std::fs::write(&path, cache.to_bytes().unwrap()).unwrap();
    let output = kagari(&["check"], &dir);
    assert!(
        stdout(&output).contains("src/main.kis:1:1: warning[W5009]: only in the cache"),
        "{}",
        stdout(&output)
    );
    let output = kagari(&["build"], &dir);
    assert_eq!(output.status.code(), Some(0), "{}", stderr(&output));
    assert!(
        stderr(&output).contains("only in the cache"),
        "{}",
        stderr(&output)
    );

    // An edit runs the analysis again and rewrites the cache.
    let mut text = std::fs::read_to_string(&main).unwrap();
    text.push_str("\n// edited\n");
    std::fs::write(&main, text).unwrap();
    let output = kagari(&["check"], &dir);
    assert_eq!(output.status.code(), Some(0), "{}", stdout(&output));
    assert!(
        !stdout(&output).contains("only in the cache"),
        "{}",
        stdout(&output)
    );
    let fresh = AnalysisCache::from_bytes(std::fs::read(&path).unwrap()).unwrap();
    assert_ne!(fresh.fingerprint, cache.fingerprint);
    assert!(
        fresh
            .diagnostics
            .iter()
            .all(|diagnostic| diagnostic.message != "only in the cache")
    );

    // An unreadable cache is replaced.
    std::fs::write(&path, b"garbage").unwrap();
    let output = kagari(&["check"], &dir);
    assert_eq!(output.status.code(), Some(0), "{}", stdout(&output));
    assert!(AnalysisCache::from_bytes(std::fs::read(&path).unwrap()).is_ok());
}

#[test]
fn clean_removes_build_outputs_and_caches_only() {
    let tmp = init_project();
    let dir = game(&tmp);
    let output = kagari(&["build", "--profile", "debug"], &dir);
    assert_eq!(output.status.code(), Some(0), "{}", stderr(&output));
    let target = dir.join("target");
    std::fs::write(target.join("v0/notes.txt"), "keep me").unwrap();

    let output = kagari(&["clean"], &dir);
    assert_eq!(output.status.code(), Some(0), "{}", stderr(&output));
    assert_eq!(
        stdout(&output),
        "removed 5 files from target\n",
        "the analysis cache, the bytecode, two archives and the debug artifact"
    );
    assert_eq!(files(&target), ["v0"]);
    assert_eq!(files(&target.join("v0")), ["notes.txt"]);

    std::fs::remove_file(target.join("v0/notes.txt")).unwrap();
    let output = kagari(&["clean"], &dir);
    assert_eq!(stdout(&output), "removed 0 files from target\n");
    assert!(!target.exists(), "empty output directories are removed");
}
