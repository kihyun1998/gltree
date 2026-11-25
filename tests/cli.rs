use std::path::Path;
use std::process::{Command, Output};

const FIXTURE: &str = "tests/fixtures/tracker.json";

fn gltree(args: &[&str]) -> Output {
    let home = std::env::temp_dir().join("gltree-no-home");
    Command::new(env!("CARGO_BIN_EXE_gltree"))
        .args(args)
        .current_dir(env!("CARGO_MANIFEST_DIR"))
        .env("HOME", &home)
        .env("USERPROFILE", &home)
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
fn tree_under_a_root_matches_its_golden() {
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
fn tree_of_every_root_leaves_out_the_families_whose_every_item_is_closed() {
    assert_eq!(
        tree_text(&["tree", "--snapshot", FIXTURE]),
        golden("tree-all.txt")
    );
}

#[test]
fn tree_all_draws_every_family() {
    assert_eq!(
        tree_text(&["tree", "--all", "--snapshot", FIXTURE]),
        golden("tree--all.txt")
    );
}

#[test]
fn tree_under_a_finished_root_still_draws_it() {
    assert_eq!(
        tree_text(&["tree", "17", "--snapshot", FIXTURE]),
        "#17 [closed] A finished feature  1/1\n  #18 [closed] Its finished slice\n"
    );
    assert_eq!(
        tree_text(&["tree", "7", "--snapshot", FIXTURE]),
        "#7 [closed] A lone piece of work\n"
    );
}

#[test]
fn all_with_a_root_number_is_a_usage_error() {
    let out = gltree(&["tree", "17", "--all", "--snapshot", FIXTURE]);
    assert_eq!(out.status.code(), Some(2));
    assert!(out.stdout.is_empty());
    let err = String::from_utf8_lossy(&out.stderr);
    assert!(err.contains("cannot be used with"), "{err}");
}

#[test]
fn json_leaves_out_the_same_families_and_all_brings_them_back() {
    let roots = |args: &[&str]| -> Vec<u64> {
        let v: serde_json::Value = serde_json::from_str(&tree_text(args)).unwrap();
        assert_eq!(v["version"], 4);
        v["roots"]
            .as_array()
            .unwrap()
            .iter()
            .map(|r| r["number"].as_u64().unwrap())
            .collect()
    };
    assert_eq!(
        roots(&["tree", "--snapshot", FIXTURE, "--json"]),
        [1, 8, 12, 14, 15]
    );
    assert_eq!(
        roots(&["tree", "--all", "--snapshot", FIXTURE, "--json"]),
        [1, 7, 8, 12, 14, 15, 17]
    );
}

#[test]
fn a_tracker_of_only_finished_families_draws_nothing_and_exits_0() {
    let dir = outside_a_clone("tree-all-finished");
    std::fs::write(
        dir.join("t.json"),
        r#"[{"iid": 1, "title": "a", "state": "closed", "desc": "+++\nchildren = [2]\n+++\n", "links": [2]},
            {"iid": 2, "title": "b", "state": "closed", "desc": "+++\nparent = 1\n+++\n", "links": [1]},
            {"iid": 3, "title": "c", "state": "closed", "desc": "+++\nparent = 99\n+++\n", "links": []}]"#,
    )
    .unwrap();
    let out = gltree_in(&dir, &["tree", "--snapshot", "t.json"]);
    assert_eq!(
        out.status.code(),
        Some(0),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
    assert!(
        out.stdout.is_empty(),
        "{}",
        String::from_utf8_lossy(&out.stdout)
    );
    let all = gltree_in(&dir, &["tree", "--all", "--snapshot", "t.json"]);
    assert_eq!(
        String::from_utf8(all.stdout).unwrap(),
        "#1 [closed] a  1/1\n  #2 [closed] b\n#3 [closed] c\n"
    );
}

#[test]
fn json_prints_the_versioned_tree() {
    let v: serde_json::Value =
        serde_json::from_str(&tree_text(&["tree", "3", "--snapshot", FIXTURE, "--json"])).unwrap();
    assert_eq!(v["version"], 4);
    assert_eq!(v["roots"][0]["blocked_by_open"], serde_json::json!([]));
    assert_eq!(
        (
            &v["roots"][0]["related"],
            &v["roots"][0]["from"],
            &v["roots"][0]["blocks"]
        ),
        (
            &serde_json::json!([]),
            &serde_json::Value::Null,
            &serde_json::json!([])
        )
    );
    assert_eq!(v["roots"][0]["number"], 3);
    assert_eq!(
        (
            v["roots"][0]["url"].clone(),
            v["roots"][0]["labels"].clone(),
            v["roots"][0]["extra"].clone()
        ),
        (
            "https://gitlab.example.com/team/app/-/issues/3".into(),
            serde_json::json!([]),
            serde_json::json!({})
        )
    );
    assert_eq!(
        v["roots"][0]["children"][0]["extra"],
        serde_json::json!({"marker": "K-1"})
    );
    assert_eq!(v["roots"][0]["children"][1]["url"], serde_json::Value::Null);
    let one: serde_json::Value =
        serde_json::from_str(&tree_text(&["tree", "1", "--snapshot", FIXTURE, "--json"])).unwrap();
    assert_eq!(
        (
            one["roots"][0]["labels"].clone(),
            one["roots"][0]["children"][0]["labels"].clone()
        ),
        (serde_json::json!(["SPEC"]), serde_json::json!(["feature"]))
    );
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
    let err = String::from_utf8_lossy(&out.stderr);
    assert!(
        err.starts_with("gltree: cannot read git's `origin` remote: "),
        "{err}"
    );
    assert!(!err.contains("cannot read the tracker"), "{err}");
}

fn clone_declaring_extra_marker(name: &str) -> std::path::PathBuf {
    let dir = clone_without_config(name);
    std::fs::write(dir.join(".gltree.toml"), "extra_keys = [\"marker\"]\n").unwrap();
    dir
}

fn clone_without_config(name: &str) -> std::path::PathBuf {
    let dir = std::env::temp_dir().join(format!("gltree-{name}"));
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
    dir
}

fn stderr_of(out: &Output) -> String {
    String::from_utf8_lossy(&out.stderr).into_owned()
}

#[test]
fn a_refused_config_is_reported_as_the_config_file_and_exits_2() {
    let dir = clone_without_config("config-refused");
    std::fs::write(dir.join(".gltree.toml"), "root_lable = \"X\"\n").unwrap();
    let out = gltree_in(&dir, &["check", "--snapshot", &fixture_path()]);
    let err = stderr_of(&out);
    assert_eq!(out.status.code(), Some(2), "{err}");
    assert!(err.starts_with("gltree: .gltree.toml: "), "{err}");
    assert!(err.contains("root_lable"), "{err}");
    assert!(
        !err.contains("snapshot") && !err.contains("tracker"),
        "{err}"
    );
    assert!(out.stdout.is_empty());
}

#[test]
fn an_unreadable_config_is_reported_as_the_config_file_and_exits_2() {
    let dir = clone_without_config("config-unreadable");
    std::fs::create_dir(dir.join(".gltree.toml")).unwrap();
    let out = gltree_in(&dir, &["check", "--snapshot", &fixture_path()]);
    let err = stderr_of(&out);
    assert_eq!(out.status.code(), Some(2), "{err}");
    assert!(err.starts_with("gltree: .gltree.toml: "), "{err}");
    assert!(
        !err.contains("snapshot") && !err.contains("tracker"),
        "{err}"
    );
    assert!(out.stdout.is_empty());
}

fn clone_with_origin(name: &str) -> std::path::PathBuf {
    let dir = clone_without_config(name);
    assert!(
        Command::new("git")
            .args([
                "remote",
                "add",
                "origin",
                "https://gitlab.example.invalid/team/app.git"
            ])
            .current_dir(&dir)
            .status()
            .unwrap()
            .success()
    );
    dir
}

fn live_tree_with_glab_config(
    dir: &Path,
    config: impl FnOnce(&Path),
) -> (Output, std::path::PathBuf) {
    let glab = dir.join("glab");
    std::fs::create_dir_all(&glab).unwrap();
    let path = glab.join("config.yml");
    config(&path);
    let out = gltree_cmd(dir, &["tree", "1"])
        .env("GLAB_CONFIG_DIR", &glab)
        .env_remove("GITLAB_TOKEN")
        .output()
        .unwrap();
    (out, path)
}

#[test]
fn a_glab_config_that_is_a_directory_is_reported_as_unreadable_not_as_holding_no_token() {
    let dir = clone_with_origin("glab-config-dir");
    let (out, path) = live_tree_with_glab_config(&dir, |p| std::fs::create_dir(p).unwrap());
    let err = stderr_of(&out);
    assert_eq!(out.status.code(), Some(2), "{err}");
    assert!(
        err.starts_with(&format!(
            "gltree: cannot read the glab config {}: ",
            path.display()
        )),
        "{err}"
    );
    assert!(!err.contains("no token"), "{err}");
    assert!(out.stdout.is_empty());
}

#[test]
fn a_glab_config_that_is_not_utf8_is_reported_as_malformed_not_as_holding_no_token() {
    let dir = clone_with_origin("glab-config-not-utf8");
    let (out, _) =
        live_tree_with_glab_config(&dir, |p| std::fs::write(p, b"\xff\xfehosts: {}\n").unwrap());
    let err = stderr_of(&out);
    assert_eq!(out.status.code(), Some(2), "{err}");
    assert!(
        err.starts_with("gltree: the glab config is malformed: "),
        "{err}"
    );
    assert!(!err.contains("no token"), "{err}");
    assert!(out.stdout.is_empty());
}

#[test]
fn an_unreadable_glab_config_fails_even_with_gitlab_token_set() {
    let dir = clone_with_origin("glab-config-dir-with-token");
    let glab = dir.join("glab");
    let path = glab.join("config.yml");
    std::fs::create_dir_all(&path).unwrap();
    let out = gltree_cmd(&dir, &["tree", "1"])
        .env("GLAB_CONFIG_DIR", &glab)
        .env("GITLAB_TOKEN", "a-token")
        .output()
        .unwrap();
    let err = stderr_of(&out);
    assert_eq!(out.status.code(), Some(2), "{err}");
    assert!(
        err.starts_with(&format!(
            "gltree: cannot read the glab config {}: ",
            path.display()
        )),
        "{err}"
    );
    assert!(!err.contains("tracker"), "{err}");
    assert!(out.stdout.is_empty());
}

fn fixture_path() -> String {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join(FIXTURE)
        .to_string_lossy()
        .into_owned()
}

#[test]
fn check_matches_its_golden_and_exits_1() {
    let dir = clone_declaring_extra_marker("check-golden");
    let out = gltree_in(&dir, &["check", "--snapshot", &fixture_path()]);
    assert_eq!(out.status.code(), Some(1));
    assert_eq!(String::from_utf8(out.stdout).unwrap(), golden("check.txt"));
}

#[test]
fn check_json_lists_each_finding() {
    let dir = clone_declaring_extra_marker("check-json");
    let out = gltree_in(&dir, &["check", "--snapshot", &fixture_path(), "--json"]);
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
    assert_eq!(numbers, ["#1", "#7", "#12", "#14", "#15", "#17"]);
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

fn gltree_cmd(dir: &Path, args: &[&str]) -> Command {
    let mut cmd = Command::new(env!("CARGO_BIN_EXE_gltree"));
    cmd.args(args)
        .current_dir(dir)
        .env("GIT_CEILING_DIRECTORIES", dir.parent().unwrap());
    at_home(&mut cmd, &dir.join(".home"));
    cmd
}

fn at_home<'a>(cmd: &'a mut Command, home: &Path) -> &'a mut Command {
    cmd.env("HOME", home).env("USERPROFILE", home)
}

fn gltree_in(dir: &Path, args: &[&str]) -> Output {
    gltree_cmd(dir, args).output().unwrap()
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

struct Served {
    child: std::process::Child,
    base: String,
}

impl Drop for Served {
    fn drop(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}

fn clone_with_config(name: &str, config: &str) -> std::path::PathBuf {
    let dir = clone_declaring_extra_marker(name);
    std::fs::write(dir.join(".gltree.toml"), config).unwrap();
    dir
}

const BADGES: &str = "extra_keys = [\"marker\"]\n\n[[badges]]\nprefix = \"SP\"\nname = \"Spec\"\nextra = \"marker\"\nmissing = \"none yet\"\n";

fn serve_in(dir: &Path, snapshot: &str) -> Served {
    serve_from(&mut gltree_cmd(
        dir,
        &["serve", "--snapshot", snapshot, "--port", "0"],
    ))
}

fn serve_from(cmd: &mut Command) -> Served {
    use std::io::BufRead;
    let mut child = cmd.stdout(std::process::Stdio::piped()).spawn().unwrap();
    let stdout = child.stdout.take().unwrap();
    let (tx, rx) = std::sync::mpsc::channel();
    std::thread::spawn(move || {
        let mut line = String::new();
        let _ = std::io::BufReader::new(stdout).read_line(&mut line);
        let _ = tx.send(line);
    });
    let mut served = Served {
        child,
        base: String::new(),
    };
    let line = rx
        .recv_timeout(std::time::Duration::from_secs(10))
        .expect("serve printed no address within 10 s");
    assert!(
        line.starts_with("http://127.0.0.1:") && line.ends_with("/\n"),
        "{line:?}"
    );
    served.base = line.trim_end().trim_end_matches('/').to_string();
    served
}

fn read_families(base: &str) -> serde_json::Value {
    let start = std::time::Instant::now();
    loop {
        let (status, v) = json_of("GET", &format!("{base}/api/families"));
        assert_eq!((status, &v["version"]), (200, &serde_json::json!(4)), "{v}");
        if v["state"] != "reading" {
            return v;
        }
        assert!(
            start.elapsed() < std::time::Duration::from_secs(30),
            "still reading: {v}"
        );
        std::thread::sleep(std::time::Duration::from_millis(20));
    }
}

fn http(method: &str, url: &str) -> (u16, String) {
    let agent: ureq::Agent = ureq::Agent::config_builder()
        .http_status_as_error(false)
        .build()
        .into();
    let mut res = match method {
        "POST" => agent.post(url).send_empty(),
        _ => agent.get(url).call(),
    }
    .unwrap();
    let status = res.status().as_u16();
    (status, res.body_mut().read_to_string().unwrap())
}

fn json_of(method: &str, url: &str) -> (u16, serde_json::Value) {
    let (status, body) = http(method, url);
    (status, serde_json::from_str(&body).unwrap())
}

fn family_numbers(v: &serde_json::Value) -> Vec<u64> {
    v["tree"]["roots"]
        .as_array()
        .unwrap()
        .iter()
        .map(|r| r["number"].as_u64().unwrap())
        .collect()
}

#[test]
fn serve_answers_the_page_every_family_with_the_badges_and_one_tree() {
    let dir = clone_with_config("serve-answers", BADGES);
    let s = serve_in(&dir, &fixture_path());
    let (status, page) = http("GET", &format!("{}/", s.base));
    assert_eq!(status, 200);
    assert!(page.contains("<title>gltree</title>"), "{page}");
    let v = read_families(&s.base);
    assert_eq!(
        (v["state"].clone(), v["tree"]["version"].clone()),
        ("ready".into(), 4.into())
    );
    assert_eq!(family_numbers(&v), [1, 7, 8, 12, 14, 15, 17]);
    assert_eq!(
        v["badges"],
        serde_json::json!([{"prefix": "SP", "name": "Spec", "extra": "marker", "missing": "none yet"}])
    );
    let (status, t) = json_of("GET", &format!("{}/api/tree/3", s.base));
    assert_eq!(status, 200);
    assert_eq!(t["version"], 4);
    assert_eq!(t["roots"][0]["number"], 3);
    assert_eq!(
        (&t["roots"][0]["related"], &t["roots"][0]["from"]),
        (&serde_json::json!([]), &serde_json::Value::Null)
    );
    assert_eq!(
        t["roots"][0]["children"][0]["extra"],
        serde_json::json!({"marker": "K-1"})
    );
    let (status, e) = json_of("GET", &format!("{}/api/tree/42", s.base));
    assert_eq!(status, 404);
    assert!(e["error"].as_str().unwrap().contains("#42"), "{e}");
    assert_eq!(
        json_of("GET", &format!("{}/api/repos", s.base)),
        (200, serde_json::json!({"version": 4, "repos": []}))
    );
}

#[test]
fn serve_refresh_reads_the_snapshot_file_again() {
    let dir = clone_with_config("serve-refresh", "");
    let snap = dir.join("t.json");
    std::fs::copy(fixture_path(), &snap).unwrap();
    let s = serve_in(&dir, "t.json");
    assert_eq!(read_families(&s.base)["state"], "ready");
    let title = |v: &serde_json::Value| {
        v["tree"]["roots"]
            .as_array()
            .unwrap()
            .iter()
            .find(|r| r["number"] == 17)
            .unwrap()["title"]
            .clone()
    };
    let text = std::fs::read_to_string(&snap).unwrap();
    assert!(text.contains("\"A finished feature\""));
    std::fs::write(
        &snap,
        text.replace("\"A finished feature\"", "\"A renamed feature\""),
    )
    .unwrap();
    let (_, before) = json_of("GET", &format!("{}/api/families", s.base));
    assert_eq!(title(&before), "A finished feature");
    let (status, after) = json_of("POST", &format!("{}/api/refresh", s.base));
    assert_eq!(status, 200);
    assert_eq!(title(&after), "A renamed feature");
    let (_, kept) = json_of("GET", &format!("{}/api/families", s.base));
    assert_eq!(title(&kept), "A renamed feature");
}

#[test]
fn serve_does_not_listen_beyond_loopback() {
    let dir = clone_with_config("serve-loopback", "");
    let s = serve_in(&dir, &fixture_path());
    let port: u16 = s.base.rsplit(':').next().unwrap().parse().unwrap();
    let probe = std::net::UdpSocket::bind("0.0.0.0:0").unwrap();
    probe
        .connect("192.0.2.1:9")
        .expect("this test needs a network address besides loopback");
    let ip = probe.local_addr().unwrap().ip();
    assert!(!ip.is_loopback() && !ip.is_unspecified(), "{ip}");
    assert_eq!(http("GET", &format!("{}/api/families", s.base)).0, 200);
    let outside = std::net::TcpStream::connect_timeout(
        &std::net::SocketAddr::new(ip, port),
        std::time::Duration::from_secs(3),
    );
    assert!(outside.is_err(), "{ip}:{port} answered");
}

fn serve_that_stops(dir: &Path, args: &[&str]) -> Output {
    let mut child = gltree_cmd(dir, &["serve"])
        .args(args)
        .args(["--port", "0"])
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped())
        .spawn()
        .unwrap();
    let start = std::time::Instant::now();
    while child.try_wait().unwrap().is_none() {
        if start.elapsed() > std::time::Duration::from_secs(30) {
            let _ = child.kill();
            let out = child.wait_with_output().unwrap();
            panic!(
                "serve kept running: {}",
                String::from_utf8_lossy(&out.stdout)
            );
        }
        std::thread::sleep(std::time::Duration::from_millis(50));
    }
    child.wait_with_output().unwrap()
}

#[test]
fn serve_refuses_a_badge_whose_extra_key_is_not_declared_and_prints_no_address() {
    let dir = clone_with_config(
        "serve-bad-badge",
        "extra_keys = [\"marker\"]\n[[badges]]\nprefix = \"SP\"\nname = \"Spec\"\nextra = \"other\"\n",
    );
    let out = serve_that_stops(&dir, &["--snapshot", &fixture_path()]);
    assert_eq!(out.status.code(), Some(2));
    assert!(out.stdout.is_empty());
    let err = String::from_utf8_lossy(&out.stderr);
    assert!(err.contains("other") && err.contains("extra_keys"), "{err}");
}

#[test]
fn serve_with_a_malformed_snapshot_prints_its_address_then_answers_failed_and_says_why() {
    use std::io::BufRead;
    use std::time::{Duration, Instant};
    let dir = clone_with_config("serve-malformed", "");
    std::fs::write(dir.join("bad.json"), "not a snapshot").unwrap();
    let mut s = serve_from(
        gltree_cmd(&dir, &["serve", "--snapshot", "bad.json", "--port", "0"])
            .stderr(std::process::Stdio::piped()),
    );
    let err = std::io::BufReader::new(s.child.stderr.take().unwrap());
    let (tx, lines) = std::sync::mpsc::channel();
    std::thread::spawn(move || {
        for line in err.lines().map_while(Result::ok) {
            if tx.send(line).is_err() {
                break;
            }
        }
    });
    let v = read_families(&s.base);
    assert_eq!(
        (&v["state"], &v["tree"]),
        (&serde_json::json!("failed"), &serde_json::Value::Null)
    );
    let why = v["error"].as_str().unwrap();
    assert!(why.contains("the snapshot file is malformed"), "{why}");
    let mut said = String::new();
    let until = Instant::now() + Duration::from_secs(10);
    while !said.contains("the first read failed") {
        let left = until.saturating_duration_since(Instant::now());
        match lines.recv_timeout(left) {
            Ok(line) => said += &(line + "\n"),
            Err(_) => break,
        }
    }
    let _ = s.child.kill();
    let _ = s.child.wait();
    assert!(
        said.contains("reading the whole tracker")
            && said.contains("the first read failed: the snapshot file is malformed"),
        "{said}"
    );
}

#[test]
fn serve_prints_its_address_while_the_first_read_still_waits_on_the_server() {
    let silent = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
    let port = silent.local_addr().unwrap().port();
    std::thread::spawn(move || {
        let held: Vec<_> = silent.incoming().collect();
        drop(held);
    });
    let dir = clone_with_origin("serve-address-first");
    let glab = dir.join("glab");
    std::fs::create_dir_all(&glab).unwrap();
    std::fs::write(
        glab.join("config.yml"),
        format!(
            "hosts:\n    gitlab.example.invalid:\n        api_host: 127.0.0.1:{port}\n        api_protocol: http\n        token: a-token\n"
        ),
    )
    .unwrap();
    let s = serve_from(
        gltree_cmd(&dir, &["serve", "--port", "0"])
            .env("GLAB_CONFIG_DIR", &glab)
            .env_remove("GITLAB_TOKEN")
            .stderr(std::process::Stdio::null()),
    );
    let base = &s.base;
    let (status, v) = json_of("GET", &format!("{base}/api/families"));
    assert_eq!(
        (status, &v["version"], &v["state"], &v["tree"]),
        (
            200,
            &serde_json::json!(4),
            &serde_json::json!("reading"),
            &serde_json::Value::Null
        ),
        "{v}"
    );
}

#[test]
fn serve_with_a_snapshot_it_cannot_read_stops_before_printing_an_address() {
    let dir = clone_with_config("serve-no-snapshot", "");
    let out = serve_that_stops(&dir, &["--snapshot", "none.json"]);
    assert_eq!(out.status.code(), Some(2));
    assert!(out.stdout.is_empty());
    assert!(String::from_utf8_lossy(&out.stderr).contains("none.json"));
}

fn help_text(args: &[&str]) -> String {
    let out = gltree(args);
    assert_eq!(out.status.code(), Some(0), "{args:?}");
    String::from_utf8(out.stdout).unwrap()
}

#[test]
fn long_help_carries_examples_requirements_and_exit_status_and_short_help_stays_short() {
    for cmd in [
        "tree", "check", "new", "edit", "move", "roots", "migrate", "sync", "show", "snapshot",
        "serve",
    ] {
        let long = help_text(&[cmd, "--help"]);
        let short = help_text(&[cmd, "-h"]);
        for heading in ["Examples:", "Requirements:", "Exit status:", "Output:"] {
            assert!(
                long.contains(heading),
                "{cmd} --help lacks {heading}\n{long}"
            );
        }
        assert!(long.contains(&format!("gltree {cmd}")), "{cmd}\n{long}");
        assert!(!long.contains("gltree.exe"), "{cmd}\n{long}");
        assert!(long.contains("glab"), "{cmd}: the login\n{long}");
        assert!(!short.contains("Examples:"), "{cmd} -h\n{short}");
        assert!(short.len() < long.len() / 2, "{cmd}: -h is not short");
    }
    for cmd in ["new", "edit", "move", "sync"] {
        let long = help_text(&[cmd, "--help"]);
        assert!(long.contains(".gltree.toml"), "{cmd}\n{long}");
    }
    assert!(help_text(&["check", "--help"]).contains("1 when it finds anything"));
}

#[test]
fn top_level_help_says_where_it_runs_and_where_to_read_more() {
    let long = help_text(&["--help"]);
    for needle in [
        "clone",
        "origin",
        "glab auth login",
        ".gltree.toml",
        "gltree <command> --help",
    ] {
        assert!(long.contains(needle), "{needle}\n{long}");
    }
    assert!(help_text(&["-h"]).len() < long.len());
}

fn toplevel(dir: &Path) -> String {
    let out = Command::new("git")
        .args(["rev-parse", "--show-toplevel"])
        .current_dir(dir)
        .output()
        .unwrap();
    assert!(out.status.success());
    String::from_utf8(out.stdout).unwrap().trim().to_string()
}

fn clone_of(name: &str, origin: &str) -> std::path::PathBuf {
    let dir = clone_without_config(name);
    assert!(
        Command::new("git")
            .args(["remote", "add", "origin", origin])
            .current_dir(&dir)
            .status()
            .unwrap()
            .success()
    );
    dir
}

fn in_home(home: &Path, args: &[&str]) -> Output {
    let mut cmd = gltree_cmd(home, args);
    at_home(&mut cmd, home).output().unwrap()
}

fn text_of(out: &Output) -> (Option<i32>, String, String) {
    (
        out.status.code(),
        String::from_utf8_lossy(&out.stdout).into_owned(),
        String::from_utf8_lossy(&out.stderr).into_owned(),
    )
}

#[test]
fn serve_add_list_and_remove_keep_the_list_in_the_home_folder() {
    let home = outside_a_clone("serve-list-home");
    let app = clone_of(
        "serve-list-app",
        "https://gitlab.example.invalid/team/app.git",
    );
    let web = clone_of(
        "serve-list-web",
        "git@gitlab.example.invalid:team/sub/web.git",
    );
    let (app_top, web_top) = (toplevel(&app), toplevel(&web));
    let app_path = app.to_string_lossy().into_owned();
    let web_path = web.to_string_lossy().into_owned();
    assert_eq!(
        text_of(&in_home(&home, &["serve", "list"])),
        (Some(0), String::new(), String::new())
    );
    assert_eq!(
        text_of(&in_home(&home, &["serve", "add", &app_path])),
        (
            Some(0),
            format!("registered app: {app_top}\n"),
            String::new()
        )
    );
    let (code, out, err) = text_of(&in_home(
        &home,
        &["serve", "add", &web_path, "--name", "app"],
    ));
    assert_eq!((code, out.as_str()), (Some(2), ""));
    assert!(err.contains("already registered as `app`"), "{err}");
    assert_eq!(
        text_of(&in_home(
            &home,
            &["serve", "add", &web_path, "--name", "site"]
        ))
        .1,
        format!("registered site: {web_top}\n")
    );
    assert_eq!(
        text_of(&in_home(&home, &["serve", "list"])).1,
        format!("app  {app_top}\nsite  {web_top}\n")
    );
    let file = home.join(".gltree").join("repos.toml");
    let kept = std::fs::read_to_string(&file).unwrap();
    assert!(kept.contains("name = \"site\""), "{kept}");
    assert_eq!(
        text_of(&in_home(&home, &["serve", "remove", "app"])),
        (Some(0), format!("removed app: {app_top}\n"), String::new())
    );
    let (code, _, err) = text_of(&in_home(&home, &["serve", "remove", "app"]));
    assert_eq!(code, Some(2));
    assert!(
        err.contains("`app`") && err.contains("registered: site"),
        "{err}"
    );
    assert_eq!(
        text_of(&in_home(&home, &["serve", "list"])).1,
        format!("site  {web_top}\n")
    );
    let (code, _, err) = text_of(&in_home(&home, &["serve", "list", "--port", "1"]));
    assert_eq!(code, Some(2), "{err}");
}

#[test]
fn serve_add_refuses_what_is_not_a_clone_of_a_gitlab_project_and_writes_nothing() {
    let home = outside_a_clone("serve-refuse-home");
    let plain = outside_a_clone("serve-refuse-plain");
    let bad = clone_of("serve-refuse-bad-origin", "notaurl");
    let bare = clone_without_config("serve-refuse-no-origin");
    let good = clone_of(
        "serve-refuse-good",
        "https://gitlab.example.invalid/team/app.git",
    );
    let gone = home.join("gone");
    for (path, name, says) in [
        (plain.as_path(), None, "is not a git clone"),
        (gone.as_path(), None, "is not a git clone"),
        (bad.as_path(), None, "names no host and project path"),
        (bare.as_path(), None, "origin"),
        (good.as_path(), Some("a b"), "cannot name a repo"),
        (good.as_path(), Some("a&b=c"), "cannot name a repo"),
    ] {
        let path = path.to_string_lossy().into_owned();
        let mut args = vec!["serve", "add", path.as_str()];
        args.extend(name.iter().flat_map(|n| ["--name", n]));
        let (code, out, err) = text_of(&in_home(&home, &args));
        assert_eq!((code, out.as_str()), (Some(2), ""), "{args:?}");
        assert!(err.contains(says), "{args:?}: {err}");
        assert!(!home.join(".gltree").exists(), "{args:?} wrote the list");
    }
}

#[test]
fn a_repo_list_that_cannot_be_read_stops_serve_before_the_address_but_not_a_snapshot() {
    let home = outside_a_clone("serve-bad-list");
    std::fs::create_dir_all(home.join(".gltree")).unwrap();
    std::fs::write(home.join(".gltree").join("repos.toml"), "not toml").unwrap();
    let mut child = gltree_cmd(&home, &["serve", "--port", "0"]);
    let out = at_home(&mut child, &home).output().unwrap();
    let (code, out, err) = text_of(&out);
    assert_eq!((code, out.as_str()), (Some(2), ""));
    assert!(
        err.contains("repos.toml") && err.contains("malformed"),
        "{err}"
    );
    let s = serve_from(at_home(
        &mut gltree_cmd(
            &home,
            &["serve", "--snapshot", &fixture_path(), "--port", "0"],
        ),
        &home,
    ));
    assert_eq!(read_families(&s.base)["state"], "ready");
}

fn node(iid: u64, state: &str, title: &str, desc: &str, links: &[u64]) -> serde_json::Value {
    serde_json::json!({
        "iid": iid.to_string(),
        "title": title,
        "state": state,
        "webUrl": format!("https://gitlab.example.invalid/x/-/issues/{iid}"),
        "widgets": [
            {"description": desc},
            {"linkedItems": {"pageInfo": {"hasNextPage": false}, "nodes":
                links.iter().map(|l| serde_json::json!({"workItem": {"iid": l.to_string()}})).collect::<Vec<_>>()}}
        ]
    })
}

fn stand_in(
    nodes: Vec<serde_json::Value>,
) -> (u16, std::sync::Arc<std::sync::atomic::AtomicUsize>) {
    let (port, hits, _) = stand_in_keeping(nodes);
    (port, hits)
}

fn stand_in_keeping(
    nodes: Vec<serde_json::Value>,
) -> (
    u16,
    std::sync::Arc<std::sync::atomic::AtomicUsize>,
    std::sync::Arc<std::sync::Mutex<Vec<String>>>,
) {
    use std::io::{BufRead, Read, Write};
    let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
    let port = listener.local_addr().unwrap().port();
    let hits = std::sync::Arc::new(std::sync::atomic::AtomicUsize::new(0));
    let counted = std::sync::Arc::clone(&hits);
    let asked = std::sync::Arc::new(std::sync::Mutex::new(Vec::new()));
    let kept = std::sync::Arc::clone(&asked);
    let body = serde_json::json!({"data": {"project": {"workItems": {
        "pageInfo": {"hasNextPage": false, "endCursor": null}, "nodes": nodes}}}})
    .to_string();
    std::thread::spawn(move || {
        for stream in listener.incoming() {
            let Ok(mut stream) = stream else { continue };
            let mut reader = std::io::BufReader::new(stream.try_clone().unwrap());
            let mut length = 0usize;
            loop {
                let mut line = String::new();
                if reader.read_line(&mut line).unwrap_or(0) == 0 || line == "\r\n" {
                    break;
                }
                if let Some((k, v)) = line.split_once(':')
                    && k.eq_ignore_ascii_case("content-length")
                {
                    length = v.trim().parse().unwrap_or(0);
                }
            }
            let mut sink = vec![0; length];
            let _ = reader.read_exact(&mut sink);
            kept.lock()
                .unwrap()
                .push(String::from_utf8_lossy(&sink).into_owned());
            counted.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
            let _ = write!(
                stream,
                "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
                body.len()
            );
        }
    });
    (port, hits, asked)
}

fn read_repos(base: &str) -> serde_json::Value {
    let start = std::time::Instant::now();
    loop {
        let (status, v) = json_of("GET", &format!("{base}/api/repos"));
        assert_eq!((status, &v["version"]), (200, &serde_json::json!(4)), "{v}");
        if v["repos"]
            .as_array()
            .unwrap()
            .iter()
            .all(|r| r["state"] != "reading")
        {
            return v;
        }
        assert!(
            start.elapsed() < std::time::Duration::from_secs(30),
            "still reading: {v}"
        );
        std::thread::sleep(std::time::Duration::from_millis(20));
    }
}

#[test]
fn serve_reads_an_item_with_the_html_gitlab_renders_for_the_repo_named() {
    let home = outside_a_clone("serve-item-home");
    let mut root = node(1, "OPEN", "app root", "+++\nchildren = []\n+++\nbody", &[]);
    root["widgets"][0]["descriptionHtml"] = serde_json::json!("<p>body</p>");
    root["widgets"]
        .as_array_mut()
        .unwrap()
        .push(serde_json::json!({"notes": {
            "pageInfo": {"hasNextPage": false, "endCursor": null},
            "nodes": [{"system": false, "internal": true, "createdAt": "2026-01-01T00:00:00Z",
                       "body": "a comment", "bodyHtml": "<p>a comment</p>",
                       "author": {"username": "mina"}}]}}));
    let (port, _, asked) = stand_in_keeping(vec![root]);
    let app = clone_of("serve-item-app", "https://app.example.invalid/team/app.git");
    let path = app.to_string_lossy().into_owned();
    assert_eq!(
        in_home(&home, &["serve", "add", path.as_str()])
            .status
            .code(),
        Some(0)
    );
    let glab = home.join("glab");
    std::fs::create_dir_all(&glab).unwrap();
    std::fs::write(
        glab.join("config.yml"),
        format!(
            "hosts:\n    app.example.invalid:\n        api_host: 127.0.0.1:{port}\n        api_protocol: http\n        token: a-token\n"
        ),
    )
    .unwrap();
    let s = serve_from(
        at_home(&mut gltree_cmd(&home, &["serve", "--port", "0"]), &home)
            .env("GLAB_CONFIG_DIR", &glab)
            .env_remove("GITLAB_TOKEN")
            .stderr(std::process::Stdio::null()),
    );
    read_repos(&s.base);
    let before = asked.lock().unwrap().len();
    let (status, v) = json_of("GET", &format!("{}/api/item/1?repo=app", s.base));
    assert_eq!(status, 200, "{v}");
    assert_eq!(
        (&v["version"], &v["number"], &v["title"], &v["body_html"]),
        (
            &serde_json::json!(4),
            &serde_json::json!(1),
            &serde_json::json!("app root"),
            &serde_json::json!("<p>body</p>")
        )
    );
    assert_eq!(
        v["comments"],
        serde_json::json!([{"author": "mina", "created_at": "2026-01-01T00:00:00Z",
            "internal": true, "body": "a comment", "body_html": "<p>a comment</p>"}])
    );
    let asked = asked.lock().unwrap();
    assert_eq!(asked.len(), before + 1);
    let query = &asked[before];
    assert!(
        query.contains("descriptionHtml") && query.contains("bodyHtml"),
        "{query}"
    );
    assert!(
        !asked[..before].iter().any(|q| q.contains("Html")),
        "the full read asked for HTML"
    );
    assert_eq!(json_of("GET", &format!("{}/api/item/1", s.base)).0, 400);
}

#[test]
fn serve_from_a_snapshot_reads_an_item_with_no_html() {
    let dir = clone_with_config("serve-item-snapshot", BADGES);
    let s = serve_in(&dir, &fixture_path());
    let (status, v) = json_of("GET", &format!("{}/api/item/1", s.base));
    assert_eq!((status, &v["number"]), (200, &serde_json::json!(1)), "{v}");
    assert!(v["body"].is_string(), "{v}");
    assert_eq!(v["body_html"], serde_json::Value::Null);
    assert_eq!(json_of("GET", &format!("{}/api/item/99999", s.base)).0, 404);
}

#[test]
fn serve_with_repos_registered_shows_each_wherever_it_runs_and_refreshes_the_one_named() {
    use std::sync::atomic::Ordering::SeqCst;
    let home = outside_a_clone("serve-registered-home");
    let (app_port, app_hits) = stand_in(vec![
        node(1, "OPEN", "app root", "+++\nchildren = [2]\n+++\n", &[2]),
        node(2, "OPEN", "app child", "+++\nparent = 1\n+++\n", &[1]),
        node(3, "CLOSED", "app done", "", &[]),
    ]);
    let (web_port, web_hits) = stand_in(vec![node(5, "OPEN", "web root", "", &[])]);
    let app = clone_of(
        "serve-registered-app",
        "https://app.example.invalid/team/app.git",
    );
    let web = clone_of(
        "serve-registered-web",
        "https://web.example.invalid/team/web.git",
    );
    let gone = clone_of(
        "serve-registered-gone",
        "https://app.example.invalid/team/gone.git",
    );
    std::fs::write(
        app.join(".gltree.toml"),
        "[[badges]]\nprefix = \"SP\"\nname = \"Spec\"\n",
    )
    .unwrap();
    for (dir, name) in [(&app, None), (&web, Some("site")), (&gone, None)] {
        let path = dir.to_string_lossy().into_owned();
        let mut args = vec!["serve", "add", path.as_str()];
        args.extend(name.iter().flat_map(|n| ["--name", n]));
        assert_eq!(in_home(&home, &args).status.code(), Some(0), "{args:?}");
    }
    std::fs::remove_dir_all(&gone).unwrap();
    let glab = home.join("glab");
    std::fs::create_dir_all(&glab).unwrap();
    std::fs::write(
        glab.join("config.yml"),
        format!(
            "hosts:\n    app.example.invalid:\n        api_host: 127.0.0.1:{app_port}\n        api_protocol: http\n        token: a-token\n    web.example.invalid:\n        api_host: 127.0.0.1:{web_port}\n        api_protocol: http\n        token: a-token\n"
        ),
    )
    .unwrap();
    let s = serve_from(
        at_home(&mut gltree_cmd(&home, &["serve", "--port", "0"]), &home)
            .env("GLAB_CONFIG_DIR", &glab)
            .env_remove("GITLAB_TOKEN")
            .stderr(std::process::Stdio::null()),
    );
    let v = read_repos(&s.base);
    let repos = v["repos"].as_array().unwrap();
    let field = |k: &str| repos.iter().map(|r| r[k].clone()).collect::<Vec<_>>();
    assert_eq!(field("name"), ["app", "site", "gone"]);
    assert_eq!(field("state"), ["ready", "ready", "failed"]);
    assert_eq!(
        field("open"),
        [
            serde_json::json!(2),
            serde_json::json!(1),
            serde_json::Value::Null
        ]
    );
    assert_eq!(
        field("families"),
        [
            serde_json::json!(2),
            serde_json::json!(1),
            serde_json::Value::Null
        ]
    );
    let why = repos[2]["error"].as_str().unwrap();
    assert!(why.contains("it is not there"), "{why}");
    let (status, f) = json_of("GET", &format!("{}/api/families?repo=site", s.base));
    assert_eq!(
        (status, &f["repo"], family_numbers(&f)),
        (200, &serde_json::json!("site"), vec![5])
    );
    let (status, f) = json_of("GET", &format!("{}/api/families?repo=app", s.base));
    assert_eq!((status, family_numbers(&f)), (200, vec![1, 3]));
    assert_eq!(f["badges"][0]["name"], "Spec");
    assert_eq!(json_of("GET", &format!("{}/api/families", s.base)).0, 400);
    assert_eq!(
        json_of("GET", &format!("{}/api/families?repo=nope", s.base)).0,
        404
    );
    let (status, g) = json_of("GET", &format!("{}/api/families?repo=gone", s.base));
    assert_eq!((status, &g["state"]), (200, &serde_json::json!("failed")));
    assert_eq!((app_hits.load(SeqCst), web_hits.load(SeqCst)), (1, 1));
    let (status, r) = json_of("POST", &format!("{}/api/refresh?repo=app", s.base));
    assert_eq!(
        (status, &r["repo"], &r["state"]),
        (200, &serde_json::json!("app"), &serde_json::json!("ready"))
    );
    assert_eq!((app_hits.load(SeqCst), web_hits.load(SeqCst)), (2, 1));
    std::fs::write(
        app.join(".gltree.toml"),
        "[[badges]]\nprefix = \"SP\"\nname = \"Specification\"\n",
    )
    .unwrap();
    let (_, f) = json_of("GET", &format!("{}/api/families?repo=app", s.base));
    assert_eq!(f["badges"][0]["name"], "Spec");
    let (status, r) = json_of("POST", &format!("{}/api/refresh?repo=app", s.base));
    assert_eq!(
        (status, &r["badges"][0]["name"]),
        (200, &serde_json::json!("Specification"))
    );
    let at = r["read_at"].clone();
    std::fs::write(app.join(".gltree.toml"), "root_lable = \"X\"\n").unwrap();
    let (status, r) = json_of("POST", &format!("{}/api/refresh?repo=app", s.base));
    let why = r["error"].as_str().unwrap();
    assert_eq!(status, 502, "{r}");
    assert!(
        why.contains("serve-registered-app")
            && why.contains(".gltree.toml: ")
            && why.contains("root_lable"),
        "{why}"
    );
    let (_, f) = json_of("GET", &format!("{}/api/families?repo=app", s.base));
    assert_eq!(
        (
            &f["state"],
            family_numbers(&f),
            &f["read_at"],
            &f["badges"][0]["name"]
        ),
        (
            &serde_json::json!("failed"),
            vec![1, 3],
            &at,
            &serde_json::json!("Specification")
        )
    );
    let (_, f) = json_of("GET", &format!("{}/api/families?repo=site", s.base));
    assert_eq!(
        (&f["state"], family_numbers(&f)),
        (&serde_json::json!("ready"), vec![5])
    );
    drop(s);
    let s = serve_in_home(&home, &["--snapshot", &fixture_path()]);
    assert_eq!(
        json_of("GET", &format!("{}/api/repos", s.base)),
        (200, serde_json::json!({"version": 4, "repos": []}))
    );
    assert_eq!(read_families(&s.base)["repo"], serde_json::Value::Null);
}

fn serve_in_home(home: &Path, args: &[&str]) -> Served {
    serve_from(
        at_home(&mut gltree_cmd(home, &["serve", "--port", "0"]), home)
            .args(args)
            .stderr(std::process::Stdio::null()),
    )
}

#[test]
fn serve_subcommands_explain_themselves() {
    for (cmd, needles) in [
        (
            "add",
            &[
                "--name",
                "repos.toml",
                "origin",
                "Examples:",
                "Output:",
                "Requirements:",
                "Exit status:",
            ][..],
        ),
        (
            "list",
            &[
                "repos.toml",
                "Examples:",
                "Output:",
                "Requirements:",
                "Exit status:",
            ][..],
        ),
        (
            "remove",
            &[
                "repos.toml",
                "Examples:",
                "Output:",
                "Requirements:",
                "Exit status:",
            ][..],
        ),
    ] {
        let long = help_text(&["serve", cmd, "--help"]);
        for n in needles {
            assert!(long.contains(n), "serve {cmd} --help lacks {n}\n{long}");
        }
        assert!(long.contains(&format!("gltree serve {cmd}")), "{long}");
        assert!(!help_text(&["serve", cmd, "-h"]).contains("Examples:"));
    }
    let serve = help_text(&["serve", "--help"]);
    assert!(!serve.contains("every command first reads"), "{serve}");
    assert!(
        serve.contains("reads each registered clone's instead"),
        "{serve}"
    );
    let top = help_text(&["--help"]);
    assert!(top.contains("registered by `gltree serve add`"), "{top}");
    for help in [
        help_text(&["serve", "--help"]),
        help_text(&["help", "serve"]),
    ] {
        for n in [
            "~/.gltree/repos.toml",
            "serve add",
            "serve list",
            "serve remove",
            "a tab",
            "shows the clone it runs in",
        ] {
            assert!(help.contains(n), "{n}\n{help}");
        }
    }
}
