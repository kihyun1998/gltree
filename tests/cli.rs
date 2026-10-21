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

#[test]
fn a_live_tree_outside_a_clone_says_where_to_run_it() {
    let dir = std::env::temp_dir().join("gltree-not-a-clone");
    std::fs::create_dir_all(&dir).unwrap();
    let out = Command::new(env!("CARGO_BIN_EXE_gltree"))
        .args(["tree", "1"])
        .current_dir(&dir)
        .env("GIT_CEILING_DIRECTORIES", dir.parent().unwrap())
        .output()
        .unwrap();
    assert_eq!(out.status.code(), Some(2));
    assert!(
        String::from_utf8_lossy(&out.stderr).contains("origin"),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
}

#[test]
fn check_matches_the_python_tool_byte_for_byte_and_exits_1() {
    let out = gltree(&["check", "--snapshot", FIXTURE]);
    assert_eq!(out.status.code(), Some(1));
    assert_eq!(String::from_utf8(out.stdout).unwrap(), golden("check.txt"));
}

#[test]
fn check_json_lists_each_finding() {
    let out = gltree(&["check", "--snapshot", FIXTURE, "--json"]);
    let v: serde_json::Value = serde_json::from_slice(&out.stdout).unwrap();
    assert_eq!(v["version"], 1);
    assert_eq!(v["findings"].as_array().unwrap().len(), 8);
    assert_eq!(v["findings"][0]["kind"], "missing-parent");
    assert_eq!(v["findings"][0]["item"], 8);
}

#[test]
fn a_clean_tracker_checks_with_exit_0() {
    let dir = std::env::temp_dir().join("gltree-clean");
    std::fs::create_dir_all(&dir).unwrap();
    let snap = dir.join("clean.json");
    std::fs::write(
        &snap,
        r#"[{"iid": 1, "title": "a", "state": "open", "desc": "", "links": [2]},
            {"iid": 2, "title": "b", "state": "open", "desc": "Parent: #1", "links": [1]}]"#,
    )
    .unwrap();
    let out = gltree(&["check", "--snapshot", snap.to_str().unwrap()]);
    assert_eq!(out.status.code(), Some(0));
    assert_eq!(String::from_utf8(out.stdout).unwrap(), "0 anomalies\n");
}

#[test]
fn an_unknown_state_policy_in_the_repo_config_stops_every_command() {
    let dir = std::env::temp_dir().join("gltree-bad-config");
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    assert!(
        Command::new("git")
            .arg("init")
            .arg("-q")
            .current_dir(&dir)
            .status()
            .unwrap()
            .success()
    );
    std::fs::write(
        dir.join(".gltree.toml"),
        "state_policy = \"every-parent\"\n",
    )
    .unwrap();
    let fixture = Path::new(env!("CARGO_MANIFEST_DIR")).join(FIXTURE);
    let out = Command::new(env!("CARGO_BIN_EXE_gltree"))
        .args(["check", "--snapshot", fixture.to_str().unwrap()])
        .current_dir(&dir)
        .output()
        .unwrap();
    assert_eq!(out.status.code(), Some(2));
    let err = String::from_utf8_lossy(&out.stderr);
    assert!(
        err.contains("every-parent") && err.contains("roots-only"),
        "{err}"
    );
}

#[test]
fn new_in_a_repo_without_a_repo_config_is_refused_before_reaching_the_server() {
    let dir = std::env::temp_dir().join("gltree-no-config");
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    assert!(
        Command::new("git")
            .args(["init", "-q"])
            .current_dir(&dir)
            .status()
            .unwrap()
            .success()
    );
    std::fs::write(dir.join("body.md"), "x").unwrap();
    let out = Command::new(env!("CARGO_BIN_EXE_gltree"))
        .args([
            "new",
            "--parent",
            "1",
            "--title",
            "t",
            "--body-file",
            "body.md",
        ])
        .current_dir(&dir)
        .output()
        .unwrap();
    assert_eq!(out.status.code(), Some(2));
    let err = String::from_utf8_lossy(&out.stderr);
    assert!(
        err.contains(".gltree.toml") && !err.contains("origin"),
        "{err}"
    );
}

#[test]
fn new_refuses_a_body_file_that_is_not_utf8() {
    let dir = std::env::temp_dir().join("gltree-bad-body");
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    std::fs::write(dir.join("body.md"), [0xff, 0xfe, 0x41]).unwrap();
    let out = Command::new(env!("CARGO_BIN_EXE_gltree"))
        .args(["new", "--title", "t", "--body-file", "body.md"])
        .current_dir(&dir)
        .output()
        .unwrap();
    assert_eq!(out.status.code(), Some(2));
    assert!(String::from_utf8_lossy(&out.stderr).contains("UTF-8"));
}

#[test]
fn move_needs_exactly_one_of_to_and_to_root() {
    for args in [
        vec!["move", "2"],
        vec!["move", "2", "--to", "1", "--to-root"],
    ] {
        let out = gltree(&args);
        assert_eq!(out.status.code(), Some(2), "{args:?}");
    }
}

#[test]
fn move_in_a_repo_without_a_repo_config_is_refused_before_reaching_the_server() {
    let dir = std::env::temp_dir().join("gltree-move-no-config");
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    assert!(
        Command::new("git")
            .args(["init", "-q"])
            .current_dir(&dir)
            .status()
            .unwrap()
            .success()
    );
    let out = Command::new(env!("CARGO_BIN_EXE_gltree"))
        .args(["move", "2", "--to-root"])
        .current_dir(&dir)
        .output()
        .unwrap();
    assert_eq!(out.status.code(), Some(2));
    let err = String::from_utf8_lossy(&out.stderr);
    assert!(
        err.contains(".gltree.toml") && !err.contains("origin"),
        "{err}"
    );
}
