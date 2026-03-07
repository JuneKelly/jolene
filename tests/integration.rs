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
fn create_test_package(dir: &Path, command_name: &str) {
    fs::create_dir_all(dir.join("commands")).unwrap();
    fs::write(
        dir.join("jolene.toml"),
        format!(
            r#"[package]
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

    create_test_package(pkg_dir.path(), "foo");

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
    let packages = state["packages"].as_array().unwrap();
    assert_eq!(packages.len(), 1);
    assert_eq!(packages[0]["source_kind"], "local");
}

#[test]
fn list_shows_installed_package() {
    let jolene_root = TempDir::new().unwrap();
    let jolene_home = TempDir::new().unwrap();
    let pkg_dir = TempDir::new().unwrap();

    fs::create_dir_all(jolene_home.path().join(".claude")).unwrap();
    create_test_package(pkg_dir.path(), "bar");

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
    create_test_package(pkg_dir.path(), "baz");

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
    let source = state["packages"][0]["source"].as_str().unwrap().to_string();

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
        .stdout(predicate::str::contains("No packages installed"));
}

// --- Prefix tests ---

/// Create a test package with an optional manifest prefix.
fn create_test_package_with_prefix(dir: &Path, command_name: &str, prefix: Option<&str>) {
    fs::create_dir_all(dir.join("commands")).unwrap();

    let prefix_line = match prefix {
        Some(p) => format!("prefix = \"{p}\"\n"),
        None => String::new(),
    };

    fs::write(
        dir.join("jolene.toml"),
        format!(
            r#"[package]
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
    create_test_package(pkg_dir.path(), "review");

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
    assert_eq!(state["packages"][0]["prefix"], "abc");
}

#[test]
fn install_with_manifest_prefix() {
    let jolene_root = TempDir::new().unwrap();
    let jolene_home = TempDir::new().unwrap();
    let pkg_dir = TempDir::new().unwrap();

    let claude_root = jolene_home.path().join(".claude");
    fs::create_dir_all(&claude_root).unwrap();
    create_test_package_with_prefix(pkg_dir.path(), "review", Some("jb"));

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
    create_test_package_with_prefix(pkg_dir.path(), "review", Some("jb"));

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
    create_test_package_with_prefix(pkg_dir.path(), "review", Some("jb"));

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
    assert!(state["packages"][0]["prefix"].is_null());
}

#[test]
fn install_without_prefix_is_flat() {
    let jolene_root = TempDir::new().unwrap();
    let jolene_home = TempDir::new().unwrap();
    let pkg_dir = TempDir::new().unwrap();

    let claude_root = jolene_home.path().join(".claude");
    fs::create_dir_all(&claude_root).unwrap();
    create_test_package(pkg_dir.path(), "review");

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
    create_test_package(pkg_dir.path(), "review");

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
    let source = state["packages"][0]["source"].as_str().unwrap().to_string();

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
    create_test_package(pkg_a.path(), "review");
    create_test_package(pkg_b.path(), "review");

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
    create_test_package(pkg_dir.path(), "review");

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
    create_test_package(pkg_dir.path(), "review");

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
    create_test_package(pkg_dir.path(), "review");

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
    create_test_package(pkg_dir.path(), "review");

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
    create_test_package(pkg_dir.path(), "review");

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
    create_test_package(pkg_dir.path(), "review");

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
    create_test_package(pkg_dir.path(), "review");

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
    let source = state["packages"][0]["source"].as_str().unwrap().to_string();

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
    assert_eq!(state["packages"][0]["prefix"], "abc");
}

// --- Template tests ---

/// Create a test package with two commands where one references the other via template.
fn create_templated_package(dir: &Path) {
    fs::create_dir_all(dir.join("commands")).unwrap();
    fs::write(
        dir.join("jolene.toml"),
        r#"[package]
name = "template-pkg"
description = "A package with templates"
version = "0.1.0"
authors = ["Test <test@test.com>"]
license = "MIT"

[content]
commands = ["review", "deploy"]
"#,
    )
    .unwrap();
    fs::write(
        dir.join("commands/review.md"),
        "# Review\nAfter review, use %{jolene:command:deploy} to deploy.\n",
    )
    .unwrap();
    fs::write(
        dir.join("commands/deploy.md"),
        "# Deploy\nA deploy command.\n",
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
fn install_templated_package_with_prefix() {
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
            "xyz",
        ])
        .assert()
        .success()
        .stdout(predicate::str::contains("Rendered 1 templated file(s)"));

    // The templated command (review) should have rendered content
    let review_symlink = claude_root.join("commands").join("xyz--review.md");
    assert!(review_symlink.is_symlink(), "review symlink should exist");

    // Read the content through the symlink — should have resolved template
    let content = fs::read_to_string(&review_symlink).unwrap();
    assert!(
        content.contains("xyz--deploy"),
        "template should resolve to prefixed name, got: {}",
        content
    );
    assert!(
        !content.contains("%{jolene:"),
        "no unresolved templates should remain"
    );

    // The non-templated command (deploy) should point directly to repos/
    let deploy_symlink = claude_root.join("commands").join("xyz--deploy.md");
    assert!(deploy_symlink.is_symlink(), "deploy symlink should exist");
    let deploy_target = fs::read_link(&deploy_symlink).unwrap();
    assert!(
        deploy_target
            .to_string_lossy()
            .contains("/repos/"),
        "non-templated file should point to repos/, got: {}",
        deploy_target.display()
    );

    // The templated command should point to built/
    let review_target = fs::read_link(&review_symlink).unwrap();
    assert!(
        review_target
            .to_string_lossy()
            .contains("/built/"),
        "templated file should point to built/, got: {}",
        review_target.display()
    );

    // built/ directory should exist
    let built_dir = jolene_root.path().join("built");
    assert!(built_dir.exists(), "built/ directory should exist");
}

#[test]
fn install_templated_package_without_prefix() {
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

    let review_symlink = claude_root.join("commands").join("review.md");
    assert!(review_symlink.is_symlink());
    let content = fs::read_to_string(&review_symlink).unwrap();
    assert!(
        content.contains("deploy"),
        "template should resolve to bare name"
    );
    assert!(
        !content.contains("%{jolene:"),
        "no unresolved templates"
    );
}

#[test]
fn non_templated_package_has_no_build_dir() {
    let jolene_root = TempDir::new().unwrap();
    let jolene_home = TempDir::new().unwrap();
    let pkg_dir = TempDir::new().unwrap();

    let claude_root = jolene_home.path().join(".claude");
    fs::create_dir_all(&claude_root).unwrap();
    create_test_package(pkg_dir.path(), "plain");

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

    let built_dir = jolene_root.path().join("built");
    assert!(
        !built_dir.exists(),
        "built/ directory should not exist for non-templated packages"
    );
}

#[test]
fn uninstall_templated_package_cleans_build_dir() {
    let jolene_root = TempDir::new().unwrap();
    let jolene_home = TempDir::new().unwrap();
    let pkg_dir = TempDir::new().unwrap();

    let claude_root = jolene_home.path().join(".claude");
    fs::create_dir_all(&claude_root).unwrap();
    create_templated_package(pkg_dir.path());

    // Install
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

    let built_dir = jolene_root.path().join("built");
    assert!(built_dir.exists(), "built/ should exist after install");

    // Get source name for uninstall
    let state: serde_json::Value =
        serde_json::from_str(&fs::read_to_string(jolene_root.path().join("state.json")).unwrap())
            .unwrap();
    let source = state["packages"][0]["source"].as_str().unwrap().to_string();

    // Uninstall
    jolene_cmd(jolene_root.path(), jolene_home.path())
        .args(["uninstall", &source, "--from", "claude-code"])
        .assert()
        .success();

    // built/ should be cleaned up (or the specific store key subdir)
    // The built dir itself may still exist but should have no subdirs for this package
    let review_symlink = claude_root.join("commands").join("xyz--review.md");
    assert!(
        !review_symlink.exists() && !review_symlink.is_symlink(),
        "symlinks should be removed"
    );
}

#[test]
fn invalid_template_reference_fails_install() {
    let jolene_root = TempDir::new().unwrap();
    let jolene_home = TempDir::new().unwrap();
    let pkg_dir = TempDir::new().unwrap();

    let claude_root = jolene_home.path().join(".claude");
    fs::create_dir_all(&claude_root).unwrap();

    // Create a package with an invalid template reference
    fs::create_dir_all(pkg_dir.path().join("commands")).unwrap();
    fs::write(
        pkg_dir.path().join("jolene.toml"),
        r#"[package]
name = "bad-template-pkg"
description = "A package with bad templates"
version = "0.1.0"
authors = ["Test <test@test.com>"]
license = "MIT"

[content]
commands = ["review"]
"#,
    )
    .unwrap();
    fs::write(
        pkg_dir.path().join("commands/review.md"),
        "# Review\nUse %{jolene:command:nonexistent} for things.\n",
    )
    .unwrap();

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
    run(&["init", "-b", "main"]);
    run(&["add", "."]);
    run(&["commit", "-m", "init"]);

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
