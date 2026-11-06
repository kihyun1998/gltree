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
    assert_eq!(v["version"], 2);
    assert_eq!(v["roots"][0]["blocked_by_open"], serde_json::json!([]));
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
    assert_eq!(v["findings"].as_array().unwrap().len(), 9);
    assert_eq!(v["findings"][0]["kind"], "old-header");
    assert_eq!(v["findings"][0]["item"], 4);
}

#[test]
fn a_clean_tracker_checks_with_exit_0() {
    let dir = std::env::temp_dir().join("gltree-clean");
    std::fs::create_dir_all(&dir).unwrap();
    let snap = dir.join("clean.json");
    std::fs::write(
        &snap,
        r#"[{"iid": 1, "title": "a", "state": "open", "desc": "+++\nchildren = [2]\n+++\n", "links": [2]},
            {"iid": 2, "title": "b", "state": "open", "desc": "+++\nparent = 1\n+++\n", "links": [1]}]"#,
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
    std::fs::write(dir.join(".gltree.toml"), "state_policy = \"leaf-only\"\n").unwrap();
    let fixture = Path::new(env!("CARGO_MANIFEST_DIR")).join(FIXTURE);
    let out = Command::new(env!("CARGO_BIN_EXE_gltree"))
        .args(["check", "--snapshot", fixture.to_str().unwrap()])
        .current_dir(&dir)
        .output()
        .unwrap();
    assert_eq!(out.status.code(), Some(2));
    let err = String::from_utf8_lossy(&out.stderr);
    assert!(
        err.contains("leaf-only") && err.contains("roots-only"),
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

#[test]
fn sync_writes_only_with_a_repo_config_and_a_dry_run_needs_none() {
    let dir = std::env::temp_dir().join("gltree-sync-no-config");
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
    let run = |args: &[&str]| {
        let out = Command::new(env!("CARGO_BIN_EXE_gltree"))
            .args(args)
            .current_dir(&dir)
            .output()
            .unwrap();
        (
            out.status.code(),
            String::from_utf8_lossy(&out.stderr).to_string(),
        )
    };
    let (code, err) = run(&["sync", "2"]);
    assert_eq!(code, Some(2));
    assert!(
        err.contains(".gltree.toml") && !err.contains("origin"),
        "{err}"
    );
    // a dry run passes the opt-in and stops only at the missing remote
    let (code, err) = run(&["sync", "2", "--dry-run"]);
    assert_eq!(code, Some(2));
    assert!(
        !err.contains(".gltree.toml") && err.contains("origin"),
        "{err}"
    );
}

#[test]
fn migrate_plans_from_a_snapshot_saves_the_result_and_the_result_checks_clean() {
    let dir = std::env::temp_dir().join("gltree-migrate");
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    let old = dir.join("old.json");
    let new = dir.join("new.json");
    std::fs::write(
        &old,
        r#"[{"iid": 1, "title": "a", "state": "open", "desc": "", "links": [2]},
            {"iid": 2, "title": "b", "state": "open", "desc": "Parent: #1\n\nFrom: #3 (seen)\n\nbody", "links": [1]},
            {"iid": 3, "title": "c", "state": "open", "desc": "", "links": []}]"#,
    )
    .unwrap();
    let out = gltree(&[
        "migrate",
        "--snapshot",
        old.to_str().unwrap(),
        "--out",
        new.to_str().unwrap(),
    ]);
    assert!(
        out.status.success(),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
    let text = String::from_utf8(out.stdout).unwrap();
    assert!(
        text.contains("  - Parent: #1") && text.contains("  + parent = 1"),
        "{text}"
    );
    assert!(
        text.ends_with("dry run: derived 1, from 1, items 2, links 1, notes 1, parent 1\n"),
        "{text}"
    );
    let out = gltree(&["check", "--snapshot", new.to_str().unwrap()]);
    assert_eq!(String::from_utf8(out.stdout).unwrap(), "0 anomalies\n");
    let again = gltree(&["migrate", "--snapshot", new.to_str().unwrap()]);
    assert!(
        String::from_utf8(again.stdout)
            .unwrap()
            .ends_with("dry run: items 0, links 0\n")
    );
    let refused = gltree(&["migrate", "--snapshot", old.to_str().unwrap(), "--apply"]);
    assert_eq!(refused.status.code(), Some(2));
}

#[test]
fn roots_lists_only_items_without_a_parent_line_with_the_filters() {
    let all = tree_text(&["roots", "--snapshot", FIXTURE]);
    let numbers: Vec<&str> = all.lines().map(|l| l.split(' ').next().unwrap()).collect();
    // #8 names a deleted parent and #14's Parent line is not its first line: only the first is a root
    assert_eq!(numbers, ["#1", "#7", "#12", "#14", "#15"]);
    let spec = tree_text(&[
        "roots",
        "--closed",
        "--label",
        "SPEC",
        "--snapshot",
        FIXTURE,
    ]);
    assert_eq!(spec, "");
    let open_spec = tree_text(&["roots", "--open", "--label", "SPEC", "--snapshot", FIXTURE]);
    assert!(
        open_spec.starts_with("#1 [open] closed:- labels:SPEC "),
        "{open_spec}"
    );
}

#[test]
fn a_date_filter_on_a_snapshot_without_closing_dates_says_so() {
    let out = gltree(&[
        "roots",
        "--closed-since",
        "2026-01-01",
        "--snapshot",
        FIXTURE,
    ]);
    assert_eq!(out.status.code(), Some(2));
    assert!(String::from_utf8_lossy(&out.stderr).contains("closed_at"));
}

const COMMENTED: &str = r#"[
  {"iid": 7, "title": "A drawer", "state": "open", "desc": "body line", "labels": ["SPEC", "bug"],
   "comments": [
     {"author": "mina", "created_at": "2026-01-02T03:04:05Z", "body": "first", "internal": false},
     {"author": null, "created_at": "2026-01-03T03:04:05Z", "body": "second\nline", "internal": true}
   ]},
  {"iid": 8, "title": "Quiet", "state": "closed", "desc": "", "comments": []},
  {"iid": 9, "title": "Old", "state": "open", "desc": "kept"}
]"#;

/// An empty directory outside any clone, so a command that reached for the server would fail.
fn outside_a_clone(name: &str) -> std::path::PathBuf {
    let dir = std::env::temp_dir().join(format!("gltree-{name}"));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    dir
}

fn gltree_in(dir: &Path, args: &[&str]) -> Output {
    Command::new(env!("CARGO_BIN_EXE_gltree"))
        .args(args)
        .current_dir(dir)
        .env("GIT_CEILING_DIRECTORIES", dir.parent().unwrap())
        .output()
        .unwrap()
}

#[test]
fn show_prints_one_item_from_a_snapshot_without_the_server() {
    let dir = outside_a_clone("show-text");
    std::fs::write(dir.join("t.json"), COMMENTED).unwrap();
    let out = gltree_in(&dir, &["show", "#7", "--snapshot", "t.json"]);
    assert!(
        out.status.success(),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
    assert_eq!(
        String::from_utf8(out.stdout).unwrap(),
        "#7 [open] A drawer\n\
         labels: SPEC, bug\n\
         \n\
         \x20 body line\n\
         \n\
         2 comments\n\
         \n\
         mina  2026-01-02T03:04:05Z\n\
         \x20 first\n\
         \n\
         unknown  2026-01-03T03:04:05Z  [internal]\n\
         \x20 second\n\
         \x20 line\n"
    );
}

#[test]
fn a_comment_cannot_pass_for_another_comments_head_or_reach_the_terminal() {
    let dir = outside_a_clone("show-forged");
    std::fs::write(
        dir.join("t.json"),
        r#"[{"iid": 1, "title": "T\u001b[2J", "state": "open", "desc": "1 comment", "comments": [
            {"author": "mina", "created_at": "2026-01-02T03:04:05Z", "internal": false,
             "body": "jo  2026-01-03T03:04:05Z  [internal]\r\n\u001b[31mred\u0007\ta\n\nlast"}
        ]}]"#,
    )
    .unwrap();
    let out = gltree_in(&dir, &["show", "1", "--snapshot", "t.json"]);
    assert_eq!(
        String::from_utf8(out.stdout).unwrap(),
        "#1 [open] T[2J\n\
         \n\
         \x20 1 comment\n\
         \n\
         1 comment\n\
         \n\
         mina  2026-01-02T03:04:05Z\n\
         \x20 jo  2026-01-03T03:04:05Z  [internal]\n\
         \x20 [31mred\ta\n\
         \n\
         \x20 last\n"
    );
    let json: serde_json::Value = serde_json::from_slice(
        &gltree_in(&dir, &["show", "1", "--snapshot", "t.json", "--json"]).stdout,
    )
    .unwrap();
    assert!(
        json["comments"][0]["body"]
            .as_str()
            .unwrap()
            .contains("\u{1b}[31mred")
    );
}

#[test]
fn show_as_json_carries_the_same_fields() {
    let dir = outside_a_clone("show-json");
    std::fs::write(dir.join("t.json"), COMMENTED).unwrap();
    let out = gltree_in(&dir, &["show", "7", "--snapshot", "t.json", "--json"]);
    let got: serde_json::Value = serde_json::from_slice(&out.stdout).unwrap();
    assert_eq!(
        got,
        serde_json::json!({
            "version": 1,
            "number": 7,
            "title": "A drawer",
            "state": "open",
            "labels": ["SPEC", "bug"],
            "body": "body line",
            "comments": [
                {"author": "mina", "created_at": "2026-01-02T03:04:05Z", "body": "first", "internal": false},
                {"author": null, "created_at": "2026-01-03T03:04:05Z", "body": "second\nline", "internal": true}
            ]
        })
    );
}

#[test]
fn show_tells_no_comments_from_comments_not_recorded() {
    let dir = outside_a_clone("show-none");
    std::fs::write(dir.join("t.json"), COMMENTED).unwrap();
    let text = |n: &str| {
        String::from_utf8(gltree_in(&dir, &["show", n, "--snapshot", "t.json"]).stdout).unwrap()
    };
    assert_eq!(text("8"), "#8 [closed] Quiet\n\nno comments\n");
    assert_eq!(
        text("9"),
        "#9 [open] Old\n\n  kept\n\ncomments not recorded; take a fresh snapshot\n"
    );
    let json = |n: &str| -> serde_json::Value {
        serde_json::from_slice(
            &gltree_in(&dir, &["show", n, "--snapshot", "t.json", "--json"]).stdout,
        )
        .unwrap()
    };
    assert_eq!(json("8")["comments"], serde_json::json!([]));
    assert_eq!(json("9")["comments"], serde_json::Value::Null);
}

#[test]
fn show_on_a_number_the_snapshot_does_not_hold_names_it() {
    let dir = outside_a_clone("show-missing");
    std::fs::write(dir.join("t.json"), COMMENTED).unwrap();
    let out = gltree_in(&dir, &["show", "42", "--snapshot", "t.json"]);
    assert_eq!(out.status.code(), Some(2));
    assert!(
        String::from_utf8_lossy(&out.stderr).contains("#42"),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
}

#[test]
fn comments_in_the_file_change_nothing_tree_check_and_roots_print() {
    let dir = outside_a_clone("comments-silent");
    let mut items: Vec<serde_json::Value> =
        serde_json::from_str(&std::fs::read_to_string(FIXTURE).unwrap()).unwrap();
    for it in &mut items {
        it["comments"] = serde_json::json!([
            {"author": "mina", "created_at": "2026-01-02T03:04:05Z", "body": "Parent: #1", "internal": false}
        ]);
    }
    let with = dir.join("with.json");
    std::fs::write(&with, serde_json::to_vec(&items).unwrap()).unwrap();
    let with = with.to_str().unwrap();
    for (cmd, extra) in [
        ("tree", vec![]),
        ("tree", vec!["1"]),
        ("tree", vec!["--json"]),
        ("check", vec![]),
        ("check", vec!["--json"]),
        ("roots", vec![]),
        ("roots", vec!["--json"]),
    ] {
        let run = |file: &str| {
            let mut args = vec![cmd, "--snapshot", file];
            args.extend(&extra);
            let out = gltree(&args);
            (out.status.code(), out.stdout)
        };
        assert_eq!(run(FIXTURE), run(with), "{cmd} {extra:?}");
    }
}

#[test]
fn migrate_out_keeps_the_comments_its_source_held() {
    let dir = outside_a_clone("migrate-comments");
    std::fs::write(dir.join("old.json"), COMMENTED).unwrap();
    let out = gltree_in(
        &dir,
        &["migrate", "--snapshot", "old.json", "--out", "new.json"],
    );
    assert!(
        out.status.success(),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
    let new: serde_json::Value =
        serde_json::from_slice(&std::fs::read(dir.join("new.json")).unwrap()).unwrap();
    let old: serde_json::Value = serde_json::from_str(COMMENTED).unwrap();
    assert_eq!(new[0]["comments"], old[0]["comments"]);
    assert_eq!(new[1]["comments"], serde_json::json!([]));
    assert!(new[2].get("comments").is_none(), "{}", new[2]);
}

#[test]
fn a_snapshot_that_cannot_read_the_server_leaves_the_previous_file_byte_for_byte() {
    let dir = outside_a_clone("snapshot-failed-fetch");
    std::fs::write(dir.join("t.json"), "the previous bytes").unwrap();
    let out = gltree_in(&dir, &["snapshot", "t.json"]);
    assert_eq!(out.status.code(), Some(2));
    assert_eq!(
        std::fs::read(dir.join("t.json")).unwrap(),
        b"the previous bytes"
    );
    let left: Vec<_> = std::fs::read_dir(&dir).unwrap().collect();
    assert_eq!(left.len(), 1);
}

#[test]
fn a_failed_migrate_out_write_leaves_no_temporary_file() {
    let dir = outside_a_clone("migrate-failed-write");
    std::fs::write(dir.join("old.json"), COMMENTED).unwrap();
    std::fs::create_dir(dir.join("taken")).unwrap();
    let out = gltree_in(
        &dir,
        &["migrate", "--snapshot", "old.json", "--out", "taken"],
    );
    assert_eq!(out.status.code(), Some(2));
    let names: Vec<String> = std::fs::read_dir(&dir)
        .unwrap()
        .map(|e| e.unwrap().file_name().into_string().unwrap())
        .collect();
    assert_eq!(names.len(), 2, "{names:?}");
}

#[test]
fn migrate_out_replaces_the_file_rather_than_writing_into_it() {
    let dir = outside_a_clone("migrate-replaces");
    std::fs::write(dir.join("old.json"), COMMENTED).unwrap();
    std::fs::write(dir.join("new.json"), "the previous bytes").unwrap();
    // A second name for the same file sees a write into it, and does not see a replacement.
    std::fs::hard_link(dir.join("new.json"), dir.join("link.json")).unwrap();
    let out = gltree_in(
        &dir,
        &["migrate", "--snapshot", "old.json", "--out", "new.json"],
    );
    assert!(out.status.success());
    assert_eq!(
        std::fs::read(dir.join("link.json")).unwrap(),
        b"the previous bytes"
    );
    assert_ne!(
        std::fs::read(dir.join("new.json")).unwrap(),
        b"the previous bytes"
    );
}

#[test]
fn a_bare_file_name_as_the_target_works_and_leaves_nothing_beside_it() {
    let dir = outside_a_clone("migrate-bare-name");
    std::fs::write(dir.join("old.json"), COMMENTED).unwrap();
    let out = gltree_in(
        &dir,
        &["migrate", "--snapshot", "old.json", "--out", "new.json"],
    );
    assert!(
        out.status.success(),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
    let mut names: Vec<String> = std::fs::read_dir(&dir)
        .unwrap()
        .map(|e| e.unwrap().file_name().into_string().unwrap())
        .collect();
    names.sort();
    assert_eq!(names, ["new.json", "old.json"]);
}
