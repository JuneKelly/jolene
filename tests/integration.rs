use std::fs;
use std::path::Path;
use std::process::Command;

use assert_cmd::assert::OutputAssertExt;
use assert_cmd::cargo::CommandCargoExt;
use predicates::prelude::*;
use tempfile::TempDir;

/// Build a `Command` for the `jolene` binary with env vars pointing to temp dirs.
fn jolene_cmd(jolene_root: &Path, jolene_home: &Path) -> Command {
    #[allow(deprecated)]
    let mut cmd = Command::cargo_bin("jolene").expect("binary exists");
    cmd.env("JOLENE_ROOT", jolene_root);
    cmd.env("JOLENE_EFFECTIVE_HOME", jolene_home);
    cmd
}

/// Create a minimal git repo with a valid `jolene.toml` and one command.
fn create_test_bundle(dir: &Path, command_name: &str) {
    fs::create_dir_all(dir.join("commands")).unwrap();
    fs::write(
        dir.join("jolene.toml"),
        format!(
            r#"[bundle]
name = "test-pkg"
description = "A test package"
version = "0.1.0"
authors = ["Test <test@test.com>"]
license = "MIT"

[content]
commands = ["{command_name}"]
"#
        ),
    )
    .unwrap();
    fs::write(
        dir.join("commands").join(format!("{command_name}.md")),
        format!("# {command_name}\nA test command.\n"),
    )
    .unwrap();

    // Initialize a git repo and commit
    let run = |args: &[&str]| {
        Command::new("git")
            .args(args)
            .current_dir(dir)
            .env("GIT_AUTHOR_NAME", "Test")
            .env("GIT_AUTHOR_EMAIL", "test@test.com")
            .env("GIT_COMMITTER_NAME", "Test")
            .env("GIT_COMMITTER_EMAIL", "test@test.com")
            .output()
            .expect("git command failed")
    };

    run(&["init", "-b", "main"]);
    run(&["add", "."]);
    run(&["commit", "-m", "init"]);
}

#[test]
fn install_local_to_claude_code() {
    let jolene_root = TempDir::new().unwrap();
    let jolene_home = TempDir::new().unwrap();
    let pkg_dir = TempDir::new().unwrap();

    // Create the target config root so claude-code is detected
    let claude_root = jolene_home.path().join(".claude");
    fs::create_dir_all(&claude_root).unwrap();

    create_test_bundle(pkg_dir.path(), "foo");

    // Install
    jolene_cmd(jolene_root.path(), jolene_home.path())
        .args([
            "install",
            "--local",
            pkg_dir.path().to_str().unwrap(),
            "--to",
            "claude-code",
        ])
        .assert()
        .success();

    // Verify symlink exists
    let symlink_path = claude_root.join("commands").join("foo.md");
    assert!(
        symlink_path.is_symlink(),
        "symlink should exist at {}",
        symlink_path.display()
    );

    // Verify symlink target is under JOLENE_ROOT
    let target = fs::read_link(&symlink_path).unwrap();
    assert!(
        target.starts_with(jolene_root.path()),
        "symlink target {} should be under JOLENE_ROOT {}",
        target.display(),
        jolene_root.path().display()
    );

    // Verify state.json exists and contains package info
    let state_path = jolene_root.path().join("state.json");
    assert!(state_path.exists(), "state.json should exist");
    let state: serde_json::Value =
        serde_json::from_str(&fs::read_to_string(&state_path).unwrap()).unwrap();
    let packages = state["bundles"].as_array().unwrap();
    assert_eq!(packages.len(), 1);
    assert_eq!(packages[0]["source_kind"], "local");
}

#[test]
fn list_shows_installed_package() {
    let jolene_root = TempDir::new().unwrap();
    let jolene_home = TempDir::new().unwrap();
    let pkg_dir = TempDir::new().unwrap();

    fs::create_dir_all(jolene_home.path().join(".claude")).unwrap();
    create_test_bundle(pkg_dir.path(), "bar");

    // Install first
    jolene_cmd(jolene_root.path(), jolene_home.path())
        .args([
            "install",
            "--local",
            pkg_dir.path().to_str().unwrap(),
            "--to",
            "claude-code",
        ])
        .assert()
        .success();

    // List
    jolene_cmd(jolene_root.path(), jolene_home.path())
        .args(["list"])
        .assert()
        .success()
        .stdout(predicate::str::contains(pkg_dir.path().to_str().unwrap()))
        .stdout(predicate::str::contains("claude-code"));
}

#[test]
fn uninstall_removes_symlinks() {
    let jolene_root = TempDir::new().unwrap();
    let jolene_home = TempDir::new().unwrap();
    let pkg_dir = TempDir::new().unwrap();

    let claude_root = jolene_home.path().join(".claude");
    fs::create_dir_all(&claude_root).unwrap();
    create_test_bundle(pkg_dir.path(), "baz");

    // Install
    jolene_cmd(jolene_root.path(), jolene_home.path())
        .args([
            "install",
            "--local",
            pkg_dir.path().to_str().unwrap(),
            "--to",
            "claude-code",
        ])
        .assert()
        .success();

    let symlink_path = claude_root.join("commands").join("baz.md");
    assert!(
        symlink_path.is_symlink(),
        "symlink should exist after install"
    );

    // Figure out the package source identifier for uninstall
    let state: serde_json::Value =
        serde_json::from_str(&fs::read_to_string(jolene_root.path().join("state.json")).unwrap())
            .unwrap();
    let source = state["bundles"][0]["source"].as_str().unwrap().to_string();

    // Uninstall
    jolene_cmd(jolene_root.path(), jolene_home.path())
        .args(["uninstall", &source, "--from", "claude-code"])
        .assert()
        .success();

    assert!(
        !symlink_path.exists() && !symlink_path.is_symlink(),
        "symlink should be removed after uninstall"
    );
}

#[test]
fn list_empty_state() {
    let jolene_root = TempDir::new().unwrap();
    let jolene_home = TempDir::new().unwrap();

    jolene_cmd(jolene_root.path(), jolene_home.path())
        .args(["list"])
        .assert()
        .success()
        .stdout(predicate::str::contains("No bundles installed"));
}

// --- Prefix tests ---

/// Create a test package with an optional manifest prefix.
fn create_test_bundle_with_prefix(dir: &Path, command_name: &str, prefix: Option<&str>) {
    fs::create_dir_all(dir.join("commands")).unwrap();

    let prefix_line = match prefix {
        Some(p) => format!("prefix = \"{p}\"\n"),
        None => String::new(),
    };

    fs::write(
        dir.join("jolene.toml"),
        format!(
            r#"[bundle]
name = "test-pkg"
{prefix_line}description = "A test package"
version = "0.1.0"
authors = ["Test <test@test.com>"]
license = "MIT"

[content]
commands = ["{command_name}"]
"#
        ),
    )
    .unwrap();
    fs::write(
        dir.join("commands").join(format!("{command_name}.md")),
        format!("# {command_name}\nA test command.\n"),
    )
    .unwrap();

    let run = |args: &[&str]| {
        Command::new("git")
            .args(args)
            .current_dir(dir)
            .env("GIT_AUTHOR_NAME", "Test")
            .env("GIT_AUTHOR_EMAIL", "test@test.com")
            .env("GIT_COMMITTER_NAME", "Test")
            .env("GIT_COMMITTER_EMAIL", "test@test.com")
            .output()
            .expect("git command failed")
    };
    run(&["init", "-b", "main"]);
    run(&["add", "."]);
    run(&["commit", "-m", "init"]);
}

#[test]
fn install_with_cli_prefix() {
    let jolene_root = TempDir::new().unwrap();
    let jolene_home = TempDir::new().unwrap();
    let pkg_dir = TempDir::new().unwrap();

    let claude_root = jolene_home.path().join(".claude");
    fs::create_dir_all(&claude_root).unwrap();
    create_test_bundle(pkg_dir.path(), "review");

    jolene_cmd(jolene_root.path(), jolene_home.path())
        .args([
            "install",
            "--local",
            pkg_dir.path().to_str().unwrap(),
            "--to",
            "claude-code",
            "--prefix",
            "abc",
        ])
        .assert()
        .success()
        .stdout(predicate::str::contains("Prefix: abc"));

    // Verify prefixed symlink
    let prefixed = claude_root.join("commands").join("abc--review.md");
    assert!(
        prefixed.is_symlink(),
        "prefixed symlink should exist at {}",
        prefixed.display()
    );

    // Verify flat symlink does NOT exist
    let flat = claude_root.join("commands").join("review.md");
    assert!(
        !flat.exists() && !flat.is_symlink(),
        "flat symlink should not exist"
    );

    // Verify state.json contains prefix
    let state: serde_json::Value =
        serde_json::from_str(&fs::read_to_string(jolene_root.path().join("state.json")).unwrap())
            .unwrap();
    assert_eq!(state["bundles"][0]["prefix"], "abc");
}

#[test]
fn install_with_manifest_prefix() {
    let jolene_root = TempDir::new().unwrap();
    let jolene_home = TempDir::new().unwrap();
    let pkg_dir = TempDir::new().unwrap();

    let claude_root = jolene_home.path().join(".claude");
    fs::create_dir_all(&claude_root).unwrap();
    create_test_bundle_with_prefix(pkg_dir.path(), "review", Some("jb"));

    jolene_cmd(jolene_root.path(), jolene_home.path())
        .args([
            "install",
            "--local",
            pkg_dir.path().to_str().unwrap(),
            "--to",
            "claude-code",
        ])
        .assert()
        .success();

    let prefixed = claude_root.join("commands").join("jb--review.md");
    assert!(
        prefixed.is_symlink(),
        "manifest-prefixed symlink should exist"
    );
}

#[test]
fn cli_prefix_overrides_manifest_prefix() {
    let jolene_root = TempDir::new().unwrap();
    let jolene_home = TempDir::new().unwrap();
    let pkg_dir = TempDir::new().unwrap();

    let claude_root = jolene_home.path().join(".claude");
    fs::create_dir_all(&claude_root).unwrap();
    create_test_bundle_with_prefix(pkg_dir.path(), "review", Some("jb"));

    jolene_cmd(jolene_root.path(), jolene_home.path())
        .args([
            "install",
            "--local",
            pkg_dir.path().to_str().unwrap(),
            "--to",
            "claude-code",
            "--prefix",
            "xyz",
        ])
        .assert()
        .success();

    // CLI prefix wins
    let xyz = claude_root.join("commands").join("xyz--review.md");
    assert!(xyz.is_symlink(), "CLI prefix should override manifest");
    let jb = claude_root.join("commands").join("jb--review.md");
    assert!(!jb.exists(), "manifest prefix should not be used");
}

#[test]
fn no_prefix_flag_overrides_manifest() {
    let jolene_root = TempDir::new().unwrap();
    let jolene_home = TempDir::new().unwrap();
    let pkg_dir = TempDir::new().unwrap();

    let claude_root = jolene_home.path().join(".claude");
    fs::create_dir_all(&claude_root).unwrap();
    create_test_bundle_with_prefix(pkg_dir.path(), "review", Some("jb"));

    jolene_cmd(jolene_root.path(), jolene_home.path())
        .args([
            "install",
            "--local",
            pkg_dir.path().to_str().unwrap(),
            "--to",
            "claude-code",
            "--no-prefix",
        ])
        .assert()
        .success();

    // Should be flat
    let flat = claude_root.join("commands").join("review.md");
    assert!(
        flat.is_symlink(),
        "flat symlink should exist with --no-prefix"
    );
    let prefixed = claude_root.join("commands").join("jb--review.md");
    assert!(!prefixed.exists(), "prefixed symlink should not exist");

    // State should have no prefix
    let state: serde_json::Value =
        serde_json::from_str(&fs::read_to_string(jolene_root.path().join("state.json")).unwrap())
            .unwrap();
    assert!(state["bundles"][0]["prefix"].is_null());
}

#[test]
fn install_without_prefix_is_flat() {
    let jolene_root = TempDir::new().unwrap();
    let jolene_home = TempDir::new().unwrap();
    let pkg_dir = TempDir::new().unwrap();

    let claude_root = jolene_home.path().join(".claude");
    fs::create_dir_all(&claude_root).unwrap();
    create_test_bundle(pkg_dir.path(), "review");

    jolene_cmd(jolene_root.path(), jolene_home.path())
        .args([
            "install",
            "--local",
            pkg_dir.path().to_str().unwrap(),
            "--to",
            "claude-code",
        ])
        .assert()
        .success();

    let flat = claude_root.join("commands").join("review.md");
    assert!(flat.is_symlink(), "flat symlink should exist");
}

#[test]
fn uninstall_removes_prefixed_symlinks() {
    let jolene_root = TempDir::new().unwrap();
    let jolene_home = TempDir::new().unwrap();
    let pkg_dir = TempDir::new().unwrap();

    let claude_root = jolene_home.path().join(".claude");
    fs::create_dir_all(&claude_root).unwrap();
    create_test_bundle(pkg_dir.path(), "review");

    // Install with prefix
    jolene_cmd(jolene_root.path(), jolene_home.path())
        .args([
            "install",
            "--local",
            pkg_dir.path().to_str().unwrap(),
            "--to",
            "claude-code",
            "--prefix",
            "abc",
        ])
        .assert()
        .success();

    let prefixed = claude_root.join("commands").join("abc--review.md");
    assert!(
        prefixed.is_symlink(),
        "prefixed symlink should exist after install"
    );

    let state: serde_json::Value =
        serde_json::from_str(&fs::read_to_string(jolene_root.path().join("state.json")).unwrap())
            .unwrap();
    let source = state["bundles"][0]["source"].as_str().unwrap().to_string();

    // Uninstall
    jolene_cmd(jolene_root.path(), jolene_home.path())
        .args(["uninstall", &source, "--from", "claude-code"])
        .assert()
        .success();

    assert!(
        !prefixed.exists() && !prefixed.is_symlink(),
        "prefixed symlink should be removed"
    );
}

#[test]
fn different_prefixes_avoid_conflict() {
    let jolene_root = TempDir::new().unwrap();
    let jolene_home = TempDir::new().unwrap();
    let pkg_a = TempDir::new().unwrap();
    let pkg_b = TempDir::new().unwrap();

    let claude_root = jolene_home.path().join(".claude");
    fs::create_dir_all(&claude_root).unwrap();

    // Both packages provide "review" command
    create_test_bundle(pkg_a.path(), "review");
    create_test_bundle(pkg_b.path(), "review");

    // Install A with prefix "abc"
    jolene_cmd(jolene_root.path(), jolene_home.path())
        .args([
            "install",
            "--local",
            pkg_a.path().to_str().unwrap(),
            "--to",
            "claude-code",
            "--prefix",
            "abc",
        ])
        .assert()
        .success();

    // Install B with prefix "xyz" — should not conflict
    jolene_cmd(jolene_root.path(), jolene_home.path())
        .args([
            "install",
            "--local",
            pkg_b.path().to_str().unwrap(),
            "--to",
            "claude-code",
            "--prefix",
            "xyz",
        ])
        .assert()
        .success();

    let abc = claude_root.join("commands").join("abc--review.md");
    let xyz = claude_root.join("commands").join("xyz--review.md");
    assert!(abc.is_symlink(), "abc--review.md should exist");
    assert!(xyz.is_symlink(), "xyz--review.md should exist");
}

#[test]
fn list_shows_prefix() {
    let jolene_root = TempDir::new().unwrap();
    let jolene_home = TempDir::new().unwrap();
    let pkg_dir = TempDir::new().unwrap();

    fs::create_dir_all(jolene_home.path().join(".claude")).unwrap();
    create_test_bundle(pkg_dir.path(), "review");

    jolene_cmd(jolene_root.path(), jolene_home.path())
        .args([
            "install",
            "--local",
            pkg_dir.path().to_str().unwrap(),
            "--to",
            "claude-code",
            "--prefix",
            "abc",
        ])
        .assert()
        .success();

    jolene_cmd(jolene_root.path(), jolene_home.path())
        .args(["list"])
        .assert()
        .success()
        .stdout(predicate::str::contains("Prefix:  abc"));
}

#[test]
fn list_omits_prefix_when_not_set() {
    let jolene_root = TempDir::new().unwrap();
    let jolene_home = TempDir::new().unwrap();
    let pkg_dir = TempDir::new().unwrap();

    fs::create_dir_all(jolene_home.path().join(".claude")).unwrap();
    create_test_bundle(pkg_dir.path(), "review");

    jolene_cmd(jolene_root.path(), jolene_home.path())
        .args([
            "install",
            "--local",
            pkg_dir.path().to_str().unwrap(),
            "--to",
            "claude-code",
        ])
        .assert()
        .success();

    jolene_cmd(jolene_root.path(), jolene_home.path())
        .args(["list"])
        .assert()
        .success()
        .stdout(predicate::str::contains("Prefix:").not());
}

#[test]
fn prefix_and_no_prefix_conflict() {
    let jolene_root = TempDir::new().unwrap();
    let jolene_home = TempDir::new().unwrap();
    let pkg_dir = TempDir::new().unwrap();

    fs::create_dir_all(jolene_home.path().join(".claude")).unwrap();
    create_test_bundle(pkg_dir.path(), "review");

    jolene_cmd(jolene_root.path(), jolene_home.path())
        .args([
            "install",
            "--local",
            pkg_dir.path().to_str().unwrap(),
            "--to",
            "claude-code",
            "--prefix",
            "abc",
            "--no-prefix",
        ])
        .assert()
        .failure()
        .stderr(predicate::str::contains("cannot be used with"));
}

#[test]
fn invalid_prefix_rejected_by_cli() {
    let jolene_root = TempDir::new().unwrap();
    let jolene_home = TempDir::new().unwrap();
    let pkg_dir = TempDir::new().unwrap();

    fs::create_dir_all(jolene_home.path().join(".claude")).unwrap();
    create_test_bundle(pkg_dir.path(), "review");

    // Uppercase is invalid
    jolene_cmd(jolene_root.path(), jolene_home.path())
        .args([
            "install",
            "--local",
            pkg_dir.path().to_str().unwrap(),
            "--to",
            "claude-code",
            "--prefix",
            "ABC",
        ])
        .assert()
        .failure()
        .stderr(predicate::str::contains(
            "lowercase letters, digits, and hyphens",
        ));

    // Consecutive hyphens are invalid (-- is the separator)
    jolene_cmd(jolene_root.path(), jolene_home.path())
        .args([
            "install",
            "--local",
            pkg_dir.path().to_str().unwrap(),
            "--to",
            "claude-code",
            "--prefix",
            "a--b",
        ])
        .assert()
        .failure()
        .stderr(predicate::str::contains("consecutive hyphens"));
}

#[test]
fn reinstall_with_different_prefix_errors() {
    let jolene_root = TempDir::new().unwrap();
    let jolene_home = TempDir::new().unwrap();
    let pkg_dir = TempDir::new().unwrap();

    let claude_root = jolene_home.path().join(".claude");
    fs::create_dir_all(&claude_root).unwrap();
    create_test_bundle(pkg_dir.path(), "review");

    // Install with prefix "abc"
    jolene_cmd(jolene_root.path(), jolene_home.path())
        .args([
            "install",
            "--local",
            pkg_dir.path().to_str().unwrap(),
            "--to",
            "claude-code",
            "--prefix",
            "abc",
        ])
        .assert()
        .success();

    // Reinstall with different prefix "xyz" — should error
    jolene_cmd(jolene_root.path(), jolene_home.path())
        .args([
            "install",
            "--local",
            pkg_dir.path().to_str().unwrap(),
            "--to",
            "claude-code",
            "--prefix",
            "xyz",
        ])
        .assert()
        .failure()
        .stderr(predicate::str::contains("already installed with prefix"));

    // Old symlink should still be intact
    let abc = claude_root.join("commands").join("abc--review.md");
    assert!(abc.is_symlink(), "original prefixed symlink should remain");

    // Reinstall with no prefix — should also error
    jolene_cmd(jolene_root.path(), jolene_home.path())
        .args([
            "install",
            "--local",
            pkg_dir.path().to_str().unwrap(),
            "--to",
            "claude-code",
        ])
        .assert()
        .failure()
        .stderr(predicate::str::contains("already installed with prefix"));
}

#[test]
fn reinstall_with_same_prefix_succeeds() {
    let jolene_root = TempDir::new().unwrap();
    let jolene_home = TempDir::new().unwrap();
    let pkg_dir = TempDir::new().unwrap();

    let claude_root = jolene_home.path().join(".claude");
    fs::create_dir_all(&claude_root).unwrap();
    create_test_bundle(pkg_dir.path(), "review");

    // Install with prefix "abc"
    jolene_cmd(jolene_root.path(), jolene_home.path())
        .args([
            "install",
            "--local",
            pkg_dir.path().to_str().unwrap(),
            "--to",
            "claude-code",
            "--prefix",
            "abc",
        ])
        .assert()
        .success();

    // Reinstall with same prefix — should succeed
    jolene_cmd(jolene_root.path(), jolene_home.path())
        .args([
            "install",
            "--local",
            pkg_dir.path().to_str().unwrap(),
            "--to",
            "claude-code",
            "--prefix",
            "abc",
        ])
        .assert()
        .success();
}

#[test]
fn update_preserves_prefix() {
    let jolene_root = TempDir::new().unwrap();
    let jolene_home = TempDir::new().unwrap();
    let pkg_dir = TempDir::new().unwrap();

    let claude_root = jolene_home.path().join(".claude");
    fs::create_dir_all(&claude_root).unwrap();
    create_test_bundle(pkg_dir.path(), "review");

    // Install with prefix
    jolene_cmd(jolene_root.path(), jolene_home.path())
        .args([
            "install",
            "--local",
            pkg_dir.path().to_str().unwrap(),
            "--to",
            "claude-code",
            "--prefix",
            "abc",
        ])
        .assert()
        .success();

    let prefixed = claude_root.join("commands").join("abc--review.md");
    assert!(
        prefixed.is_symlink(),
        "prefixed symlink should exist after install"
    );

    // Add a new commit to the package so update has something to pull
    let run = |args: &[&str]| {
        Command::new("git")
            .args(args)
            .current_dir(pkg_dir.path())
            .env("GIT_AUTHOR_NAME", "Test")
            .env("GIT_AUTHOR_EMAIL", "test@test.com")
            .env("GIT_COMMITTER_NAME", "Test")
            .env("GIT_COMMITTER_EMAIL", "test@test.com")
            .output()
            .expect("git command failed")
    };
    fs::write(
        pkg_dir.path().join("commands").join("review.md"),
        "# review\nUpdated content.\n",
    )
    .unwrap();
    run(&["add", "."]);
    run(&["commit", "-m", "update content"]);

    // Update
    let state: serde_json::Value =
        serde_json::from_str(&fs::read_to_string(jolene_root.path().join("state.json")).unwrap())
            .unwrap();
    let source = state["bundles"][0]["source"].as_str().unwrap().to_string();

    jolene_cmd(jolene_root.path(), jolene_home.path())
        .args(["update", &source])
        .assert()
        .success();

    // Prefixed symlink should still exist
    assert!(
        prefixed.is_symlink(),
        "prefixed symlink should survive update"
    );

    // Flat symlink should NOT exist
    let flat = claude_root.join("commands").join("review.md");
    assert!(
        !flat.exists() && !flat.is_symlink(),
        "flat symlink should not appear after update"
    );

    // State should still have the prefix
    let state: serde_json::Value =
        serde_json::from_str(&fs::read_to_string(jolene_root.path().join("state.json")).unwrap())
            .unwrap();
    assert_eq!(state["bundles"][0]["prefix"], "abc");
}

// --- Template tests ---

/// Helper: run git commands in a directory.
fn git_in(dir: &Path, args: &[&str]) {
    Command::new("git")
        .args(args)
        .current_dir(dir)
        .env("GIT_AUTHOR_NAME", "Test")
        .env("GIT_AUTHOR_EMAIL", "test@test.com")
        .env("GIT_COMMITTER_NAME", "Test")
        .env("GIT_COMMITTER_EMAIL", "test@test.com")
        .output()
        .expect("git command failed");
}

/// Create a test package with one command and one skill, both using template expressions.
/// The command references `jolene.resolve("deploy")` and `jolene.vars.doc_url`.
/// The skill references `jolene.target` and `jolene.prefix`.
fn create_templated_package(dir: &Path) {
    fs::create_dir_all(dir.join("commands")).unwrap();
    fs::create_dir_all(dir.join("skills/analysis")).unwrap();

    fs::write(
        dir.join("jolene.toml"),
        r#"[bundle]
name = "tmpl-pkg"
description = "A templated test package"
version = "1.0.0"
authors = ["Test <test@test.com>"]
license = "MIT"

[content]
commands = ["deploy"]
skills = ["analysis"]

[template.vars]
doc_url = "https://example.com/docs"
show_advanced = false
max_retries = 3
"#,
    )
    .unwrap();

    // Command with template expressions referencing resolve and vars.
    fs::write(
        dir.join("commands/deploy.md"),
        r#"# Deploy

Run this command to deploy.

Docs: {~ jolene.vars.doc_url ~}
Retries: {~ jolene.vars.max_retries ~}

{%~ if jolene.vars.show_advanced ~%}
Advanced: enabled
{%~ endif ~%}
"#,
    )
    .unwrap();

    // Skill with template expressions referencing target and prefix.
    fs::write(
        dir.join("skills/analysis/SKILL.md"),
        r#"# Analysis Skill

Target: {~ jolene.target ~}
Prefix: {~ jolene.prefix ~}
Bundle: {~ jolene.bundle.name ~} v{~ jolene.bundle.version ~}
Deploy command: {~ jolene.resolve("deploy") ~}
"#,
    )
    .unwrap();

    git_in(dir, &["init", "-b", "main"]);
    git_in(dir, &["add", "."]);
    git_in(dir, &["commit", "-m", "init"]);
}

#[test]
fn install_templated_package_renders_expressions() {
    let jolene_root = TempDir::new().unwrap();
    let jolene_home = TempDir::new().unwrap();
    let pkg_dir = TempDir::new().unwrap();

    let claude_root = jolene_home.path().join(".claude");
    fs::create_dir_all(&claude_root).unwrap();
    create_templated_package(pkg_dir.path());

    jolene_cmd(jolene_root.path(), jolene_home.path())
        .args([
            "install",
            "--local",
            pkg_dir.path().to_str().unwrap(),
            "--to",
            "claude-code",
        ])
        .assert()
        .success();

    // Command symlink should point to rendered/, not repos/
    let cmd_link = claude_root.join("commands/deploy.md");
    assert!(cmd_link.is_symlink(), "command symlink should exist");
    let cmd_target = fs::read_link(&cmd_link).unwrap();
    assert!(
        cmd_target.to_string_lossy().contains("rendered"),
        "templated command should point to rendered/: {}",
        cmd_target.display()
    );

    // Read rendered command content — should have interpolated values.
    let content = fs::read_to_string(&cmd_link).unwrap();
    assert!(
        content.contains("https://example.com/docs"),
        "rendered command should contain doc_url value"
    );
    assert!(
        content.contains("Retries: 3"),
        "rendered command should contain max_retries value"
    );
    // show_advanced is false, so "Advanced: enabled" should NOT appear
    assert!(
        !content.contains("Advanced: enabled"),
        "conditional block should be excluded when show_advanced=false"
    );

    // Skill symlink should also point to rendered/
    let skill_link = claude_root.join("skills/analysis");
    assert!(skill_link.is_symlink(), "skill symlink should exist");
    let skill_target = fs::read_link(&skill_link).unwrap();
    assert!(
        skill_target.to_string_lossy().contains("rendered"),
        "templated skill should point to rendered/: {}",
        skill_target.display()
    );

    // Read rendered SKILL.md — should have target and package info.
    let skill_content = fs::read_to_string(skill_link.join("SKILL.md")).unwrap();
    assert!(
        skill_content.contains("Target: claude-code"),
        "rendered skill should contain target slug"
    );
    assert!(
        skill_content.contains("Bundle: tmpl-pkg v1.0.0"),
        "rendered skill should contain package info"
    );
    assert!(
        skill_content.contains("Deploy command: deploy"),
        "rendered skill should resolve deploy command (no prefix)"
    );
    assert!(
        !skill_content.contains("Deploy command: deploy.md"),
        "resolve() should not include .md extension"
    );
}

#[test]
fn install_templated_with_prefix_resolves_correctly() {
    let jolene_root = TempDir::new().unwrap();
    let jolene_home = TempDir::new().unwrap();
    let pkg_dir = TempDir::new().unwrap();

    let claude_root = jolene_home.path().join(".claude");
    fs::create_dir_all(&claude_root).unwrap();
    create_templated_package(pkg_dir.path());

    jolene_cmd(jolene_root.path(), jolene_home.path())
        .args([
            "install",
            "--local",
            pkg_dir.path().to_str().unwrap(),
            "--to",
            "claude-code",
            "--prefix",
            "acme",
        ])
        .assert()
        .success();

    // Command should be at prefixed path.
    let cmd_link = claude_root.join("commands/acme--deploy.md");
    assert!(cmd_link.is_symlink(), "prefixed command symlink should exist");

    // Skill's SKILL.md should show resolve() with prefix applied.
    let skill_link = claude_root.join("skills/acme--analysis");
    assert!(skill_link.is_symlink(), "prefixed skill symlink should exist");

    let skill_content = fs::read_to_string(skill_link.join("SKILL.md")).unwrap();
    assert!(
        skill_content.contains("Deploy command: acme--deploy"),
        "resolve() should apply prefix: got:\n{}",
        skill_content
    );
    assert!(
        !skill_content.contains("Deploy command: acme--deploy.md"),
        "resolve() should not include .md extension"
    );
    assert!(
        skill_content.contains("Prefix: acme"),
        "jolene.prefix should be 'acme'"
    );
}

#[test]
fn install_templated_with_var_overrides() {
    let jolene_root = TempDir::new().unwrap();
    let jolene_home = TempDir::new().unwrap();
    let pkg_dir = TempDir::new().unwrap();

    let claude_root = jolene_home.path().join(".claude");
    fs::create_dir_all(&claude_root).unwrap();
    create_templated_package(pkg_dir.path());

    jolene_cmd(jolene_root.path(), jolene_home.path())
        .args([
            "install",
            "--local",
            pkg_dir.path().to_str().unwrap(),
            "--to",
            "claude-code",
            "--var",
            "doc_url=https://internal.corp/docs",
            "--var",
            "show_advanced=true",
        ])
        .assert()
        .success();

    let content = fs::read_to_string(claude_root.join("commands/deploy.md")).unwrap();
    assert!(
        content.contains("https://internal.corp/docs"),
        "overridden doc_url should appear in rendered output"
    );
    assert!(
        content.contains("Advanced: enabled"),
        "show_advanced=true should enable the conditional block"
    );

    // Verify overrides are stored in state.
    let state: serde_json::Value =
        serde_json::from_str(&fs::read_to_string(jolene_root.path().join("state.json")).unwrap())
            .unwrap();
    let overrides = &state["bundles"][0]["var_overrides"];
    assert!(
        !overrides.is_null(),
        "var_overrides should be stored in state"
    );
    assert_eq!(
        overrides["doc_url"],
        "https://internal.corp/docs"
    );
    assert_eq!(overrides["show_advanced"], true);
}

#[test]
fn install_templated_with_vars_json() {
    let jolene_root = TempDir::new().unwrap();
    let jolene_home = TempDir::new().unwrap();
    let pkg_dir = TempDir::new().unwrap();

    let claude_root = jolene_home.path().join(".claude");
    fs::create_dir_all(&claude_root).unwrap();
    create_templated_package(pkg_dir.path());

    jolene_cmd(jolene_root.path(), jolene_home.path())
        .args([
            "install",
            "--local",
            pkg_dir.path().to_str().unwrap(),
            "--to",
            "claude-code",
            "--vars-json",
            r#"{"doc_url": "https://override.com", "max_retries": 5}"#,
        ])
        .assert()
        .success();

    let content = fs::read_to_string(claude_root.join("commands/deploy.md")).unwrap();
    assert!(
        content.contains("https://override.com"),
        "vars-json doc_url should appear"
    );
    assert!(
        content.contains("Retries: 5"),
        "vars-json max_retries should appear"
    );
}

#[test]
fn install_templated_var_unknown_key_errors() {
    let jolene_root = TempDir::new().unwrap();
    let jolene_home = TempDir::new().unwrap();
    let pkg_dir = TempDir::new().unwrap();

    fs::create_dir_all(jolene_home.path().join(".claude")).unwrap();
    create_templated_package(pkg_dir.path());

    jolene_cmd(jolene_root.path(), jolene_home.path())
        .args([
            "install",
            "--local",
            pkg_dir.path().to_str().unwrap(),
            "--to",
            "claude-code",
            "--var",
            "nonexistent_key=value",
        ])
        .assert()
        .failure()
        .stderr(predicate::str::contains("not declared in [template.vars]"));
}

#[test]
fn install_templated_var_type_mismatch_errors() {
    let jolene_root = TempDir::new().unwrap();
    let jolene_home = TempDir::new().unwrap();
    let pkg_dir = TempDir::new().unwrap();

    fs::create_dir_all(jolene_home.path().join(".claude")).unwrap();
    create_templated_package(pkg_dir.path());

    // show_advanced is declared as bool, passing a string should fail.
    jolene_cmd(jolene_root.path(), jolene_home.path())
        .args([
            "install",
            "--local",
            pkg_dir.path().to_str().unwrap(),
            "--to",
            "claude-code",
            "--var",
            "show_advanced=notabool",
        ])
        .assert()
        .failure()
        .stderr(predicate::str::contains("declared as bool"));
}

#[test]
fn install_templated_vars_json_null_errors() {
    let jolene_root = TempDir::new().unwrap();
    let jolene_home = TempDir::new().unwrap();
    let pkg_dir = TempDir::new().unwrap();

    fs::create_dir_all(jolene_home.path().join(".claude")).unwrap();
    create_templated_package(pkg_dir.path());

    jolene_cmd(jolene_root.path(), jolene_home.path())
        .args([
            "install",
            "--local",
            pkg_dir.path().to_str().unwrap(),
            "--to",
            "claude-code",
            "--vars-json",
            r#"{"doc_url": null}"#,
        ])
        .assert()
        .failure()
        .stderr(predicate::str::contains("null"));
}

#[test]
fn install_templated_vars_json_not_object_errors() {
    let jolene_root = TempDir::new().unwrap();
    let jolene_home = TempDir::new().unwrap();
    let pkg_dir = TempDir::new().unwrap();

    fs::create_dir_all(jolene_home.path().join(".claude")).unwrap();
    create_templated_package(pkg_dir.path());

    jolene_cmd(jolene_root.path(), jolene_home.path())
        .args([
            "install",
            "--local",
            pkg_dir.path().to_str().unwrap(),
            "--to",
            "claude-code",
            "--vars-json",
            r#""just a string""#,
        ])
        .assert()
        .failure()
        .stderr(predicate::str::contains("expected a JSON object"));
}

#[test]
fn install_templated_resolve_unknown_item_errors() {
    let jolene_root = TempDir::new().unwrap();
    let jolene_home = TempDir::new().unwrap();
    let pkg_dir = TempDir::new().unwrap();

    fs::create_dir_all(jolene_home.path().join(".claude")).unwrap();

    // Create a package where the command references a nonexistent item.
    fs::create_dir_all(pkg_dir.path().join("commands")).unwrap();
    fs::write(
        pkg_dir.path().join("jolene.toml"),
        r#"[bundle]
name = "bad-resolve"
description = "Test"
version = "1.0.0"
authors = ["Test <test@test.com>"]
license = "MIT"

[content]
commands = ["cmd"]
"#,
    )
    .unwrap();
    fs::write(
        pkg_dir.path().join("commands/cmd.md"),
        "Invoke: {~ jolene.resolve(\"nonexistent\") ~}\n",
    )
    .unwrap();
    git_in(pkg_dir.path(), &["init", "-b", "main"]);
    git_in(pkg_dir.path(), &["add", "."]);
    git_in(pkg_dir.path(), &["commit", "-m", "init"]);

    jolene_cmd(jolene_root.path(), jolene_home.path())
        .args([
            "install",
            "--local",
            pkg_dir.path().to_str().unwrap(),
            "--to",
            "claude-code",
        ])
        .assert()
        .failure()
        .stderr(predicate::str::contains("nonexistent"));
}

#[test]
fn install_templated_ambiguous_resolve_errors() {
    let jolene_root = TempDir::new().unwrap();
    let jolene_home = TempDir::new().unwrap();
    let pkg_dir = TempDir::new().unwrap();

    fs::create_dir_all(jolene_home.path().join(".claude")).unwrap();

    // Create a package where "review" is both a command and a skill.
    fs::create_dir_all(pkg_dir.path().join("commands")).unwrap();
    fs::create_dir_all(pkg_dir.path().join("skills/review")).unwrap();
    fs::write(
        pkg_dir.path().join("jolene.toml"),
        r#"[bundle]
name = "ambiguous"
description = "Test"
version = "1.0.0"
authors = ["Test <test@test.com>"]
license = "MIT"

[content]
commands = ["review"]
skills = ["review"]
"#,
    )
    .unwrap();
    // The command itself tries to resolve "review" without disambiguating.
    fs::write(
        pkg_dir.path().join("commands/review.md"),
        "See: {~ jolene.resolve(\"review\") ~}\n",
    )
    .unwrap();
    fs::write(
        pkg_dir.path().join("skills/review/SKILL.md"),
        "# Review Skill\n",
    )
    .unwrap();
    git_in(pkg_dir.path(), &["init", "-b", "main"]);
    git_in(pkg_dir.path(), &["add", "."]);
    git_in(pkg_dir.path(), &["commit", "-m", "init"]);

    jolene_cmd(jolene_root.path(), jolene_home.path())
        .args([
            "install",
            "--local",
            pkg_dir.path().to_str().unwrap(),
            "--to",
            "claude-code",
        ])
        .assert()
        .failure()
        .stderr(predicate::str::contains("ambiguous"));
}

#[test]
fn install_templated_ambiguous_resolve_with_disambiguator_succeeds() {
    let jolene_root = TempDir::new().unwrap();
    let jolene_home = TempDir::new().unwrap();
    let pkg_dir = TempDir::new().unwrap();

    let claude_root = jolene_home.path().join(".claude");
    fs::create_dir_all(&claude_root).unwrap();

    // Create a package where "review" is both a command and a skill,
    // but the template uses the disambiguator.
    fs::create_dir_all(pkg_dir.path().join("commands")).unwrap();
    fs::create_dir_all(pkg_dir.path().join("skills/review")).unwrap();
    fs::write(
        pkg_dir.path().join("jolene.toml"),
        r#"[bundle]
name = "disambiguous"
description = "Test"
version = "1.0.0"
authors = ["Test <test@test.com>"]
license = "MIT"

[content]
commands = ["review"]
skills = ["review"]
"#,
    )
    .unwrap();
    fs::write(
        pkg_dir.path().join("commands/review.md"),
        "See skill: {~ jolene.resolve(\"review\", \"skill\") ~}\n",
    )
    .unwrap();
    fs::write(
        pkg_dir.path().join("skills/review/SKILL.md"),
        "See command: {~ jolene.resolve(\"review\", \"command\") ~}\n",
    )
    .unwrap();
    git_in(pkg_dir.path(), &["init", "-b", "main"]);
    git_in(pkg_dir.path(), &["add", "."]);
    git_in(pkg_dir.path(), &["commit", "-m", "init"]);

    jolene_cmd(jolene_root.path(), jolene_home.path())
        .args([
            "install",
            "--local",
            pkg_dir.path().to_str().unwrap(),
            "--to",
            "claude-code",
        ])
        .assert()
        .success();

    let cmd_content = fs::read_to_string(claude_root.join("commands/review.md")).unwrap();
    assert!(
        cmd_content.contains("See skill: review"),
        "disambiguated resolve to skill should return skill name: got:\n{}",
        cmd_content
    );

    let skill_content =
        fs::read_to_string(claude_root.join("skills/review/SKILL.md")).unwrap();
    assert!(
        skill_content.contains("See command: review"),
        "disambiguated resolve to command should return name: got:\n{}",
        skill_content
    );
    assert!(
        !skill_content.contains("See command: review.md"),
        "resolve() should not include .md extension"
    );
}

#[test]
fn install_non_templated_files_symlink_to_repos() {
    let jolene_root = TempDir::new().unwrap();
    let jolene_home = TempDir::new().unwrap();
    let pkg_dir = TempDir::new().unwrap();

    let claude_root = jolene_home.path().join(".claude");
    fs::create_dir_all(&claude_root).unwrap();

    // Create a package with NO template expressions — should symlink to repos/.
    create_test_bundle(pkg_dir.path(), "plain");

    jolene_cmd(jolene_root.path(), jolene_home.path())
        .args([
            "install",
            "--local",
            pkg_dir.path().to_str().unwrap(),
            "--to",
            "claude-code",
        ])
        .assert()
        .success();

    let link = claude_root.join("commands/plain.md");
    assert!(link.is_symlink(), "symlink should exist");
    let target = fs::read_link(&link).unwrap();
    assert!(
        target.to_string_lossy().contains("repos"),
        "non-templated file should point to repos/: {}",
        target.display()
    );
    assert!(
        !target.to_string_lossy().contains("rendered"),
        "non-templated file should NOT point to rendered/"
    );
}

#[test]
fn install_templated_per_target_rendering() {
    let jolene_root = TempDir::new().unwrap();
    let jolene_home = TempDir::new().unwrap();
    let pkg_dir = TempDir::new().unwrap();

    // Create both claude-code and codex target dirs.
    let claude_root = jolene_home.path().join(".claude");
    let codex_root = jolene_home.path().join(".codex");
    fs::create_dir_all(&claude_root).unwrap();
    fs::create_dir_all(&codex_root).unwrap();

    // Create a package with a skill that uses jolene.target in a conditional.
    fs::create_dir_all(pkg_dir.path().join("skills/info")).unwrap();
    fs::write(
        pkg_dir.path().join("jolene.toml"),
        r#"[bundle]
name = "multi-target"
description = "Test"
version = "1.0.0"
authors = ["Test <test@test.com>"]
license = "MIT"

[content]
skills = ["info"]
"#,
    )
    .unwrap();
    fs::write(
        pkg_dir.path().join("skills/info/SKILL.md"),
        r#"# Info
Target: {~ jolene.target ~}
{%~ if jolene.target == "claude-code" ~%}
Claude-specific instructions here.
{%~ elif jolene.target == "codex" ~%}
Codex-specific instructions here.
{%~ endif ~%}
"#,
    )
    .unwrap();
    git_in(pkg_dir.path(), &["init", "-b", "main"]);
    git_in(pkg_dir.path(), &["add", "."]);
    git_in(pkg_dir.path(), &["commit", "-m", "init"]);

    jolene_cmd(jolene_root.path(), jolene_home.path())
        .args([
            "install",
            "--local",
            pkg_dir.path().to_str().unwrap(),
            "--to",
            "claude-code",
            "--to",
            "codex",
        ])
        .assert()
        .success();

    // Claude-code version should have claude-specific content.
    let claude_content =
        fs::read_to_string(claude_root.join("skills/info/SKILL.md")).unwrap();
    assert!(
        claude_content.contains("Target: claude-code"),
        "claude-code target should appear"
    );
    assert!(
        claude_content.contains("Claude-specific"),
        "claude-code conditional should be included"
    );
    assert!(
        !claude_content.contains("Codex-specific"),
        "codex conditional should NOT be included for claude-code"
    );

    // Codex version should have codex-specific content.
    let codex_content =
        fs::read_to_string(codex_root.join("skills/info/SKILL.md")).unwrap();
    assert!(
        codex_content.contains("Target: codex"),
        "codex target should appear"
    );
    assert!(
        codex_content.contains("Codex-specific"),
        "codex conditional should be included"
    );
    assert!(
        !codex_content.contains("Claude-specific"),
        "claude conditional should NOT be included for codex"
    );
}

#[test]
fn update_re_renders_templated_content() {
    let jolene_root = TempDir::new().unwrap();
    let jolene_home = TempDir::new().unwrap();
    let pkg_dir = TempDir::new().unwrap();

    let claude_root = jolene_home.path().join(".claude");
    fs::create_dir_all(&claude_root).unwrap();
    create_templated_package(pkg_dir.path());

    // Install with a var override.
    jolene_cmd(jolene_root.path(), jolene_home.path())
        .args([
            "install",
            "--local",
            pkg_dir.path().to_str().unwrap(),
            "--to",
            "claude-code",
            "--var",
            "doc_url=https://v1.docs.com",
        ])
        .assert()
        .success();

    let content_v1 = fs::read_to_string(claude_root.join("commands/deploy.md")).unwrap();
    assert!(content_v1.contains("https://v1.docs.com"));

    // Update the package source — change the template to include more text.
    fs::write(
        pkg_dir.path().join("commands/deploy.md"),
        r#"# Deploy v2

Updated deploy. Docs: {~ jolene.vars.doc_url ~}
"#,
    )
    .unwrap();
    git_in(pkg_dir.path(), &["add", "."]);
    git_in(pkg_dir.path(), &["commit", "-m", "v2"]);

    // Run update.
    let state: serde_json::Value =
        serde_json::from_str(&fs::read_to_string(jolene_root.path().join("state.json")).unwrap())
            .unwrap();
    let source = state["bundles"][0]["source"].as_str().unwrap().to_string();

    jolene_cmd(jolene_root.path(), jolene_home.path())
        .args(["update", &source])
        .assert()
        .success();

    // Rendered content should reflect the updated template AND preserved override.
    let content_v2 = fs::read_to_string(claude_root.join("commands/deploy.md")).unwrap();
    assert!(
        content_v2.contains("Deploy v2"),
        "updated template text should appear"
    );
    assert!(
        content_v2.contains("https://v1.docs.com"),
        "stored var override should be preserved across update"
    );
}

#[test]
fn uninstall_purge_removes_rendered() {
    let jolene_root = TempDir::new().unwrap();
    let jolene_home = TempDir::new().unwrap();
    let pkg_dir = TempDir::new().unwrap();

    let claude_root = jolene_home.path().join(".claude");
    fs::create_dir_all(&claude_root).unwrap();
    create_templated_package(pkg_dir.path());

    jolene_cmd(jolene_root.path(), jolene_home.path())
        .args([
            "install",
            "--local",
            pkg_dir.path().to_str().unwrap(),
            "--to",
            "claude-code",
        ])
        .assert()
        .success();

    // Verify rendered/ dir exists.
    let rendered_root = jolene_root.path().join("rendered");
    assert!(rendered_root.exists(), "rendered/ should exist after install");

    let state: serde_json::Value =
        serde_json::from_str(&fs::read_to_string(jolene_root.path().join("state.json")).unwrap())
            .unwrap();
    let source = state["bundles"][0]["source"].as_str().unwrap().to_string();

    // Uninstall with --purge.
    jolene_cmd(jolene_root.path(), jolene_home.path())
        .args(["uninstall", &source, "--purge"])
        .assert()
        .success()
        .stdout(predicate::str::contains("Purged rendered copies"));

    // Rendered/ should be cleaned up.
    // The rendered root might still exist as an empty dir, but the hash subdir should be gone.
    let rendered_entries: Vec<_> = if rendered_root.exists() {
        fs::read_dir(&rendered_root)
            .unwrap()
            .filter_map(|e| e.ok())
            .collect()
    } else {
        vec![]
    };
    assert!(
        rendered_entries.is_empty(),
        "rendered/ should be empty after purge"
    );
}

#[test]
fn doctor_detects_orphaned_rendered() {
    let jolene_root = TempDir::new().unwrap();
    let jolene_home = TempDir::new().unwrap();

    // Create an orphaned rendered/ directory (no corresponding state entry).
    let orphan_dir = jolene_root.path().join("rendered/deadbeef1234567890abcdef1234567890abcdef1234567890abcdef12345678");
    fs::create_dir_all(&orphan_dir).unwrap();

    // Write an empty state.json so doctor has something to work with.
    fs::write(
        jolene_root.path().join("state.json"),
        r#"{"bundles":[]}"#,
    )
    .unwrap();

    jolene_cmd(jolene_root.path(), jolene_home.path())
        .args(["doctor"])
        .assert()
        .success()
        .stdout(predicate::str::contains("ORPHANED RENDERED"));
}

#[test]
fn install_templated_no_vars_section_works() {
    let jolene_root = TempDir::new().unwrap();
    let jolene_home = TempDir::new().unwrap();
    let pkg_dir = TempDir::new().unwrap();

    let claude_root = jolene_home.path().join(".claude");
    fs::create_dir_all(&claude_root).unwrap();

    // Package with template expressions but no [template.vars] — only uses
    // jolene.prefix, jolene.target, jolene.resolve().
    fs::create_dir_all(pkg_dir.path().join("commands")).unwrap();
    fs::write(
        pkg_dir.path().join("jolene.toml"),
        r#"[bundle]
name = "no-vars"
description = "Test"
version = "1.0.0"
authors = ["Test <test@test.com>"]
license = "MIT"

[content]
commands = ["cmd"]
"#,
    )
    .unwrap();
    fs::write(
        pkg_dir.path().join("commands/cmd.md"),
        "Target: {~ jolene.target ~}\nSelf: {~ jolene.resolve(\"cmd\") ~}\n",
    )
    .unwrap();
    git_in(pkg_dir.path(), &["init", "-b", "main"]);
    git_in(pkg_dir.path(), &["add", "."]);
    git_in(pkg_dir.path(), &["commit", "-m", "init"]);

    jolene_cmd(jolene_root.path(), jolene_home.path())
        .args([
            "install",
            "--local",
            pkg_dir.path().to_str().unwrap(),
            "--to",
            "claude-code",
        ])
        .assert()
        .success();

    let content = fs::read_to_string(claude_root.join("commands/cmd.md")).unwrap();
    assert!(content.contains("Target: claude-code"));
    assert!(content.contains("Self: cmd"));
    assert!(!content.contains("Self: cmd.md"), "resolve() should not include .md extension");
}

#[test]
fn install_templated_fuel_limit_errors() {
    let jolene_root = TempDir::new().unwrap();
    let jolene_home = TempDir::new().unwrap();
    let pkg_dir = TempDir::new().unwrap();

    fs::create_dir_all(jolene_home.path().join(".claude")).unwrap();

    // Create a package with a template that will exhaust fuel.
    fs::create_dir_all(pkg_dir.path().join("commands")).unwrap();
    fs::write(
        pkg_dir.path().join("jolene.toml"),
        r#"[bundle]
name = "fuel-hog"
description = "Test"
version = "1.0.0"
authors = ["Test <test@test.com>"]
license = "MIT"

[content]
commands = ["hog"]
"#,
    )
    .unwrap();
    // A huge for-loop to blow through fuel. MiniJinja with 50k fuel should
    // not allow iterating through a very large inline range.
    // We use nested loops to exceed fuel quickly.
    fs::write(
        pkg_dir.path().join("commands/hog.md"),
        r#"{%~ for i in range(10000) ~%}{%~ for j in range(10000) ~%}x{%~ endfor ~%}{%~ endfor ~%}"#,
    )
    .unwrap();
    git_in(pkg_dir.path(), &["init", "-b", "main"]);
    git_in(pkg_dir.path(), &["add", "."]);
    git_in(pkg_dir.path(), &["commit", "-m", "init"]);

    jolene_cmd(jolene_root.path(), jolene_home.path())
        .args([
            "install",
            "--local",
            pkg_dir.path().to_str().unwrap(),
            "--to",
            "claude-code",
        ])
        .assert()
        .failure()
        .stderr(predicate::str::contains("exceeded execution limit").or(
            predicate::str::contains("Template error").or(
                predicate::str::contains("out of fuel")
            )
        ));
}

#[test]
fn install_templated_skill_with_mixed_files() {
    let jolene_root = TempDir::new().unwrap();
    let jolene_home = TempDir::new().unwrap();
    let pkg_dir = TempDir::new().unwrap();

    let claude_root = jolene_home.path().join(".claude");
    fs::create_dir_all(&claude_root).unwrap();

    // Create a skill with: SKILL.md (templated), config.txt (plain), nested/data.md (plain).
    fs::create_dir_all(pkg_dir.path().join("skills/mixed/nested")).unwrap();
    fs::write(
        pkg_dir.path().join("jolene.toml"),
        r#"[bundle]
name = "mixed-skill"
description = "Test"
version = "1.0.0"
authors = ["Test <test@test.com>"]
license = "MIT"

[content]
skills = ["mixed"]
"#,
    )
    .unwrap();
    fs::write(
        pkg_dir.path().join("skills/mixed/SKILL.md"),
        "# Mixed\nTarget: {~ jolene.target ~}\n",
    )
    .unwrap();
    fs::write(
        pkg_dir.path().join("skills/mixed/config.txt"),
        "plain config content\nno templates here\n",
    )
    .unwrap();
    fs::write(
        pkg_dir.path().join("skills/mixed/nested/data.md"),
        "nested plain file\n",
    )
    .unwrap();
    git_in(pkg_dir.path(), &["init", "-b", "main"]);
    git_in(pkg_dir.path(), &["add", "."]);
    git_in(pkg_dir.path(), &["commit", "-m", "init"]);

    jolene_cmd(jolene_root.path(), jolene_home.path())
        .args([
            "install",
            "--local",
            pkg_dir.path().to_str().unwrap(),
            "--to",
            "claude-code",
        ])
        .assert()
        .success();

    // Skill dir should be symlinked to rendered/ (because SKILL.md is templated).
    let skill_link = claude_root.join("skills/mixed");
    assert!(skill_link.is_symlink(), "skill symlink should exist");
    let skill_target = fs::read_link(&skill_link).unwrap();
    assert!(
        skill_target.to_string_lossy().contains("rendered"),
        "skill should point to rendered/ because SKILL.md is templated"
    );

    // SKILL.md should be rendered.
    let skill_md = fs::read_to_string(skill_link.join("SKILL.md")).unwrap();
    assert!(
        skill_md.contains("Target: claude-code"),
        "SKILL.md should be rendered"
    );

    // config.txt should be copied as-is (no expressions).
    let config = fs::read_to_string(skill_link.join("config.txt")).unwrap();
    assert_eq!(
        config, "plain config content\nno templates here\n",
        "config.txt should be copied verbatim"
    );

    // Nested file should also be copied.
    let nested = fs::read_to_string(skill_link.join("nested/data.md")).unwrap();
    assert_eq!(
        nested, "nested plain file\n",
        "nested plain file should be copied verbatim"
    );
}

#[test]
fn update_switches_symlink_when_templated_status_changes() {
    let jolene_root = TempDir::new().unwrap();
    let jolene_home = TempDir::new().unwrap();
    let pkg_dir = TempDir::new().unwrap();

    let claude_root = jolene_home.path().join(".claude");
    fs::create_dir_all(&claude_root).unwrap();

    // Start with a templated command.
    fs::create_dir_all(pkg_dir.path().join("commands")).unwrap();
    fs::write(
        pkg_dir.path().join("jolene.toml"),
        r#"[bundle]
name = "flip-pkg"
description = "Test"
version = "1.0.0"
authors = ["Test <test@test.com>"]
license = "MIT"

[content]
commands = ["flip"]
"#,
    )
    .unwrap();
    fs::write(
        pkg_dir.path().join("commands/flip.md"),
        "# Flip\nTarget: {~ jolene.target ~}\n",
    )
    .unwrap();
    git_in(pkg_dir.path(), &["init", "-b", "main"]);
    git_in(pkg_dir.path(), &["add", "."]);
    git_in(pkg_dir.path(), &["commit", "-m", "v1 templated"]);

    // Install — should point to rendered/.
    jolene_cmd(jolene_root.path(), jolene_home.path())
        .args([
            "install",
            "--local",
            pkg_dir.path().to_str().unwrap(),
            "--to",
            "claude-code",
        ])
        .assert()
        .success();

    let cmd_link = claude_root.join("commands/flip.md");
    let target_v1 = fs::read_link(&cmd_link).unwrap();
    assert!(
        target_v1.to_string_lossy().contains("rendered"),
        "v1: should point to rendered/"
    );
    let content_v1 = fs::read_to_string(&cmd_link).unwrap();
    assert!(
        content_v1.contains("Target: claude-code"),
        "v1: should be rendered"
    );

    // Now the author removes template expressions.
    fs::write(
        pkg_dir.path().join("commands/flip.md"),
        "# Flip\nPlain content, no templates anymore.\n",
    )
    .unwrap();
    git_in(pkg_dir.path(), &["add", "."]);
    git_in(pkg_dir.path(), &["commit", "-m", "v2 plain"]);

    // Update.
    let state: serde_json::Value =
        serde_json::from_str(&fs::read_to_string(jolene_root.path().join("state.json")).unwrap())
            .unwrap();
    let source = state["bundles"][0]["source"].as_str().unwrap().to_string();

    jolene_cmd(jolene_root.path(), jolene_home.path())
        .args(["update", &source])
        .assert()
        .success();

    // After update, symlink should now point to repos/ (no longer templated).
    let target_v2 = fs::read_link(&cmd_link).unwrap();
    assert!(
        target_v2.to_string_lossy().contains("repos"),
        "v2: should point to repos/ after template removal, got: {}",
        target_v2.display()
    );
    assert!(
        !target_v2.to_string_lossy().contains("rendered"),
        "v2: should NOT point to rendered/"
    );

    let content_v2 = fs::read_to_string(&cmd_link).unwrap();
    assert!(
        content_v2.contains("Plain content, no templates anymore"),
        "v2: should show updated plain content"
    );
}

#[test]
fn update_switches_symlink_when_item_becomes_templated() {
    let jolene_root = TempDir::new().unwrap();
    let jolene_home = TempDir::new().unwrap();
    let pkg_dir = TempDir::new().unwrap();

    let claude_root = jolene_home.path().join(".claude");
    fs::create_dir_all(&claude_root).unwrap();

    // Start with a plain command (no template expressions).
    fs::create_dir_all(pkg_dir.path().join("commands")).unwrap();
    fs::write(
        pkg_dir.path().join("jolene.toml"),
        r#"[bundle]
name = "gain-tmpl"
description = "Test"
version = "1.0.0"
authors = ["Test <test@test.com>"]
license = "MIT"

[content]
commands = ["cmd"]
"#,
    )
    .unwrap();
    fs::write(
        pkg_dir.path().join("commands/cmd.md"),
        "# Cmd\nPlain content.\n",
    )
    .unwrap();
    git_in(pkg_dir.path(), &["init", "-b", "main"]);
    git_in(pkg_dir.path(), &["add", "."]);
    git_in(pkg_dir.path(), &["commit", "-m", "v1 plain"]);

    // Install — should point to repos/.
    jolene_cmd(jolene_root.path(), jolene_home.path())
        .args([
            "install",
            "--local",
            pkg_dir.path().to_str().unwrap(),
            "--to",
            "claude-code",
        ])
        .assert()
        .success();

    let cmd_link = claude_root.join("commands/cmd.md");
    let target_v1 = fs::read_link(&cmd_link).unwrap();
    assert!(
        target_v1.to_string_lossy().contains("repos"),
        "v1: should point to repos/"
    );

    // Author adds template expressions.
    fs::write(
        pkg_dir.path().join("commands/cmd.md"),
        "# Cmd\nNow with target: {~ jolene.target ~}\n",
    )
    .unwrap();
    git_in(pkg_dir.path(), &["add", "."]);
    git_in(pkg_dir.path(), &["commit", "-m", "v2 templated"]);

    let state: serde_json::Value =
        serde_json::from_str(&fs::read_to_string(jolene_root.path().join("state.json")).unwrap())
            .unwrap();
    let source = state["bundles"][0]["source"].as_str().unwrap().to_string();

    jolene_cmd(jolene_root.path(), jolene_home.path())
        .args(["update", &source])
        .assert()
        .success();

    // After update, symlink should now point to rendered/.
    let target_v2 = fs::read_link(&cmd_link).unwrap();
    assert!(
        target_v2.to_string_lossy().contains("rendered"),
        "v2: should point to rendered/ after gaining templates, got: {}",
        target_v2.display()
    );

    let content_v2 = fs::read_to_string(&cmd_link).unwrap();
    assert!(
        content_v2.contains("Now with target: claude-code"),
        "v2: should be rendered with target value"
    );
}

#[test]
fn install_templated_vars_json_partial_deep_merge() {
    let jolene_root = TempDir::new().unwrap();
    let jolene_home = TempDir::new().unwrap();
    let pkg_dir = TempDir::new().unwrap();

    let claude_root = jolene_home.path().join(".claude");
    fs::create_dir_all(&claude_root).unwrap();

    // Package with a nested object variable.
    fs::create_dir_all(pkg_dir.path().join("commands")).unwrap();
    fs::write(
        pkg_dir.path().join("jolene.toml"),
        r#"[bundle]
name = "merge-pkg"
description = "Test"
version = "1.0.0"
authors = ["Test <test@test.com>"]
license = "MIT"

[content]
commands = ["info"]

[template.vars]
api = { base_url = "https://api.example.com", version = "v2", timeout = 30 }
"#,
    )
    .unwrap();

    fs::write(
        pkg_dir.path().join("commands/info.md"),
        r#"# Info
Endpoint: {~ jolene.vars.api.base_url ~}/{~ jolene.vars.api.version ~}
Timeout: {~ jolene.vars.api.timeout ~}
"#,
    )
    .unwrap();

    git_in(pkg_dir.path(), &["init", "-b", "main"]);
    git_in(pkg_dir.path(), &["add", "."]);
    git_in(pkg_dir.path(), &["commit", "-m", "init"]);

    // Override only base_url — version and timeout should be preserved.
    jolene_cmd(jolene_root.path(), jolene_home.path())
        .args([
            "install",
            "--local",
            pkg_dir.path().to_str().unwrap(),
            "--to",
            "claude-code",
            "--vars-json",
            r#"{"api": {"base_url": "https://api.internal.corp"}}"#,
        ])
        .assert()
        .success();

    let content = fs::read_to_string(claude_root.join("commands/info.md")).unwrap();
    assert!(
        content.contains("Endpoint: https://api.internal.corp/v2"),
        "base_url should be overridden, version preserved: got:\n{}",
        content
    );
    assert!(
        content.contains("Timeout: 30"),
        "timeout should be preserved from defaults: got:\n{}",
        content
    );
    assert!(
        !content.contains("api.example.com"),
        "original base_url should NOT appear"
    );
}

#[test]
fn install_templated_skill_with_binary_file() {
    let jolene_root = TempDir::new().unwrap();
    let jolene_home = TempDir::new().unwrap();
    let pkg_dir = TempDir::new().unwrap();

    let claude_root = jolene_home.path().join(".claude");
    fs::create_dir_all(&claude_root).unwrap();

    // Skill with: SKILL.md (templated), icon.png (binary).
    fs::create_dir_all(pkg_dir.path().join("skills/with-bin")).unwrap();
    fs::write(
        pkg_dir.path().join("jolene.toml"),
        r#"[bundle]
name = "bin-skill"
description = "Test"
version = "1.0.0"
authors = ["Test <test@test.com>"]
license = "MIT"

[content]
skills = ["with-bin"]
"#,
    )
    .unwrap();

    fs::write(
        pkg_dir.path().join("skills/with-bin/SKILL.md"),
        "# With Bin\nTarget: {~ jolene.target ~}\n",
    )
    .unwrap();

    // Write a small binary file (PNG header + garbage — not valid UTF-8).
    let binary_content: Vec<u8> = vec![
        0x89, 0x50, 0x4E, 0x47, 0x0D, 0x0A, 0x1A, 0x0A, // PNG magic
        0x00, 0x00, 0x00, 0x0D, 0x49, 0x48, 0x44, 0x52, // IHDR chunk
        0xFF, 0xFE, 0xFD, 0xFC, 0xFB, 0xFA, 0xF9, 0xF8, // invalid UTF-8
    ];
    fs::write(
        pkg_dir.path().join("skills/with-bin/icon.png"),
        &binary_content,
    )
    .unwrap();

    git_in(pkg_dir.path(), &["init", "-b", "main"]);
    git_in(pkg_dir.path(), &["add", "."]);
    git_in(pkg_dir.path(), &["commit", "-m", "init"]);

    jolene_cmd(jolene_root.path(), jolene_home.path())
        .args([
            "install",
            "--local",
            pkg_dir.path().to_str().unwrap(),
            "--to",
            "claude-code",
        ])
        .assert()
        .success();

    // Skill should be symlinked to rendered/ (SKILL.md is templated).
    let skill_link = claude_root.join("skills/with-bin");
    assert!(skill_link.is_symlink(), "skill symlink should exist");
    let skill_target = fs::read_link(&skill_link).unwrap();
    assert!(
        skill_target.to_string_lossy().contains("rendered"),
        "skill should point to rendered/"
    );

    // SKILL.md should be rendered.
    let skill_md = fs::read_to_string(skill_link.join("SKILL.md")).unwrap();
    assert!(
        skill_md.contains("Target: claude-code"),
        "SKILL.md should be rendered"
    );

    // Binary file should be copied byte-for-byte.
    let copied_binary = fs::read(skill_link.join("icon.png")).unwrap();
    assert_eq!(
        copied_binary, binary_content,
        "binary file should be copied exactly as-is"
    );
}

#[test]
fn update_aborts_when_stored_override_var_removed() {
    let jolene_root = TempDir::new().unwrap();
    let jolene_home = TempDir::new().unwrap();
    let pkg_dir = TempDir::new().unwrap();

    let claude_root = jolene_home.path().join(".claude");
    fs::create_dir_all(&claude_root).unwrap();

    // v1: package has doc_url variable.
    fs::create_dir_all(pkg_dir.path().join("commands")).unwrap();
    fs::write(
        pkg_dir.path().join("jolene.toml"),
        r#"[bundle]
name = "evolve-pkg"
description = "Test"
version = "1.0.0"
authors = ["Test <test@test.com>"]
license = "MIT"

[content]
commands = ["cmd"]

[template.vars]
doc_url = "https://example.com"
max_retries = 3
"#,
    )
    .unwrap();
    fs::write(
        pkg_dir.path().join("commands/cmd.md"),
        "Docs: {~ jolene.vars.doc_url ~}\nRetries: {~ jolene.vars.max_retries ~}\n",
    )
    .unwrap();

    git_in(pkg_dir.path(), &["init", "-b", "main"]);
    git_in(pkg_dir.path(), &["add", "."]);
    git_in(pkg_dir.path(), &["commit", "-m", "v1"]);

    // Install with overrides for both vars.
    jolene_cmd(jolene_root.path(), jolene_home.path())
        .args([
            "install",
            "--local",
            pkg_dir.path().to_str().unwrap(),
            "--to",
            "claude-code",
            "--var",
            "doc_url=https://internal.corp",
            "--var",
            "max_retries=5",
        ])
        .assert()
        .success();

    // v2: author removes doc_url from [template.vars].
    fs::write(
        pkg_dir.path().join("jolene.toml"),
        r#"[bundle]
name = "evolve-pkg"
description = "Test"
version = "2.0.0"
authors = ["Test <test@test.com>"]
license = "MIT"

[content]
commands = ["cmd"]

[template.vars]
max_retries = 3
"#,
    )
    .unwrap();
    fs::write(
        pkg_dir.path().join("commands/cmd.md"),
        "Retries: {~ jolene.vars.max_retries ~}\n",
    )
    .unwrap();

    git_in(pkg_dir.path(), &["add", "."]);
    git_in(pkg_dir.path(), &["commit", "-m", "v2 removed doc_url"]);

    // Update should fail because stored override for doc_url no longer exists.
    let state: serde_json::Value =
        serde_json::from_str(&fs::read_to_string(jolene_root.path().join("state.json")).unwrap())
            .unwrap();
    let source = state["bundles"][0]["source"].as_str().unwrap().to_string();

    jolene_cmd(jolene_root.path(), jolene_home.path())
        .args(["update", &source])
        .assert()
        .failure()
        .stderr(predicate::str::contains("doc_url"));
}

/// Package with a command that contains literal Jolene delimiter text, excluded
/// from template rendering via `[template] exclude`.
fn create_package_with_excluded_item(dir: &Path) {
    fs::create_dir_all(dir.join("commands")).unwrap();
    fs::write(
        dir.join("jolene.toml"),
        r#"[bundle]
name = "syntax-docs"
description = "Package with literal template delimiter text"
version = "1.0.0"
authors = ["test"]
license = "MIT"

[content]
commands = ["guide"]

[template]
exclude = ["guide"]
"#,
    )
    .unwrap();
    // The command contains literal {~ ... ~} text — without the exclude flag this
    // would cause a MiniJinja syntax error at install time.
    fs::write(
        dir.join("commands/guide.md"),
        "Use {~ jolene.resolve(\"name\") ~} syntax in your templates.\n",
    )
    .unwrap();
    git_in(dir, &["init", "-b", "main"]);
    git_in(dir, &["add", "."]);
    git_in(dir, &["commit", "-m", "init"]);
}

#[test]
fn excluded_item_with_delimiters_installs_without_error() {
    let jolene_root = TempDir::new().unwrap();
    let jolene_home = TempDir::new().unwrap();
    let pkg_dir = TempDir::new().unwrap();

    let claude_root = jolene_home.path().join(".claude");
    fs::create_dir_all(&claude_root).unwrap();
    create_package_with_excluded_item(pkg_dir.path());

    jolene_cmd(jolene_root.path(), jolene_home.path())
        .args([
            "install",
            "--local",
            pkg_dir.path().to_str().unwrap(),
            "--to",
            "claude-code",
        ])
        .assert()
        .success();

    // Symlink should point to repos/, not rendered/ (excluded from rendering).
    let cmd_link = claude_root.join("commands/guide.md");
    assert!(cmd_link.is_symlink(), "command symlink should exist");
    let cmd_target = fs::read_link(&cmd_link).unwrap();
    assert!(
        !cmd_target.to_string_lossy().contains("rendered"),
        "excluded item should point to repos/, not rendered: {}",
        cmd_target.display()
    );

    // File content should be the literal text, not rendered output.
    let content = fs::read_to_string(&cmd_link).unwrap();
    assert!(
        content.contains("{~ jolene.resolve("),
        "excluded item should contain literal delimiter text: {content}"
    );
}

#[test]
fn exclude_unknown_name_rejects_install() {
    let jolene_root = TempDir::new().unwrap();
    let jolene_home = TempDir::new().unwrap();
    let pkg_dir = TempDir::new().unwrap();

    let claude_root = jolene_home.path().join(".claude");
    fs::create_dir_all(&claude_root).unwrap();

    fs::create_dir_all(pkg_dir.path().join("commands")).unwrap();
    fs::write(
        pkg_dir.path().join("jolene.toml"),
        r#"[bundle]
name = "test"
description = "test"
version = "1.0.0"
authors = ["test"]
license = "MIT"

[content]
commands = ["deploy"]

[template]
exclude = ["nonexistent"]
"#,
    )
    .unwrap();
    fs::write(pkg_dir.path().join("commands/deploy.md"), "# deploy").unwrap();
    git_in(pkg_dir.path(), &["init", "-b", "main"]);
    git_in(pkg_dir.path(), &["add", "."]);
    git_in(pkg_dir.path(), &["commit", "-m", "init"]);

    jolene_cmd(jolene_root.path(), jolene_home.path())
        .args([
            "install",
            "--local",
            pkg_dir.path().to_str().unwrap(),
            "--to",
            "claude-code",
        ])
        .assert()
        .failure()
        .stderr(predicate::str::contains("[template.exclude]"))
        .stderr(predicate::str::contains("nonexistent"));
}

#[test]
fn state_json_packages_key_migrated_to_bundles_on_mutating_command() {
    let jolene_root = TempDir::new().unwrap();
    let jolene_home = TempDir::new().unwrap();
    let pkg_dir = TempDir::new().unwrap();

    let claude_root = jolene_home.path().join(".claude");
    fs::create_dir_all(&claude_root).unwrap();
    create_test_bundle(pkg_dir.path(), "review");

    // Install so state.json is created with the new "bundles" key.
    jolene_cmd(jolene_root.path(), jolene_home.path())
        .args([
            "install",
            "--local",
            pkg_dir.path().to_str().unwrap(),
            "--to",
            "claude-code",
        ])
        .assert()
        .success();

    let state_path = jolene_root.path().join("state.json");

    // Rewrite state.json replacing the top-level "bundles" key with the old "packages" key.
    let content = fs::read_to_string(&state_path).unwrap();
    let mut raw: serde_json::Value = serde_json::from_str(&content).unwrap();
    let bundles = raw.as_object_mut().unwrap().remove("bundles").unwrap();
    raw.as_object_mut().unwrap().insert("packages".to_string(), bundles);
    fs::write(&state_path, serde_json::to_string(&raw).unwrap()).unwrap();

    // Run a mutating command (update). It should migrate and report it.
    jolene_cmd(jolene_root.path(), jolene_home.path())
        .args(["update"])
        .assert()
        .success()
        .stderr(predicate::str::contains("Migrating state.json"));

    // After migration, state.json should use "bundles", not "packages".
    let after = fs::read_to_string(&state_path).unwrap();
    assert!(
        after.contains("\"bundles\""),
        "state.json should use \"bundles\" after migration"
    );
    assert!(
        !after.contains("\"packages\""),
        "state.json should not contain old \"packages\" key after migration"
    );
}
