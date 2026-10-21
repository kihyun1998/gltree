//! The binary against golden output the consumers' Python tree tool printed for the same snapshot.

use std::path::Path;
use std::process::{Command, Output};

const FIXTURE: &str = "tests/fixtures/tracker.json";

fn gltree(args: &[&str]) -> Output {
    Command::new(env!("CARGO_BIN_EXE_gltree"))
        .args(args)
        .current_dir(env!("CARGO_MANIFEST_DIR"))
        .output()
        .unwrap()
}

fn golden(name: &str) -> String {
    std::fs::read_to_string(
        Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("tests/golden")
            .join(name),
    )
    .unwrap()
}

fn tree_text(args: &[&str]) -> String {
    let out = gltree(args);
    assert!(
        out.status.success(),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
    String::from_utf8(out.stdout).unwrap()
}

#[test]
fn tree_under_a_root_matches_the_python_tool_byte_for_byte() {
    for (root, file) in [
        ("1", "tree-1.txt"),
        ("#3", "tree-3.txt"),
        ("9", "tree-9.txt"),
    ] {
        assert_eq!(
            tree_text(&["tree", root, "--snapshot", FIXTURE]),
            golden(file),
            "tree {root}"
        );
    }
}

#[test]
fn tree_of_every_root_matches_the_python_tool_byte_for_byte() {
    assert_eq!(
        tree_text(&["tree", "--snapshot", FIXTURE]),
        golden("tree-all.txt")
    );
}

#[test]
fn json_prints_the_versioned_tree() {
    let v: serde_json::Value =
        serde_json::from_str(&tree_text(&["tree", "3", "--snapshot", FIXTURE, "--json"])).unwrap();
    assert_eq!(v["version"], 1);
    assert_eq!(v["roots"][0]["number"], 3);
    assert_eq!(
        (
            v["roots"][0]["closed"].clone(),
            v["roots"][0]["total"].clone()
        ),
        (1.into(), 2.into())
    );
    assert_eq!(
        v["roots"][0]["children"][1]["title"],
        "A large font keeps its labels apart"
    );
}

#[test]
fn an_unknown_root_or_a_missing_snapshot_exits_2_with_a_message() {
    for args in [
        vec!["tree", "42", "--snapshot", FIXTURE],
        vec!["tree", "1", "--snapshot", "tests/fixtures/none.json"],
    ] {
        let out = gltree(&args);
        assert_eq!(out.status.code(), Some(2), "{args:?}");
        assert!(
            String::from_utf8_lossy(&out.stderr).starts_with("gltree: "),
            "{args:?}"
        );
    }
}
