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
        .args(["install", "--local", pkg_dir.path().to_str().unwrap(), "--to", "claude-code"])
        .assert()
        .success();

    // Verify symlink exists
    let symlink_path = claude_root.join("commands").join("foo.md");
    assert!(symlink_path.is_symlink(), "symlink should exist at {}", symlink_path.display());

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
        .args(["install", "--local", pkg_dir.path().to_str().unwrap(), "--to", "claude-code"])
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
        .args(["install", "--local", pkg_dir.path().to_str().unwrap(), "--to", "claude-code"])
        .assert()
        .success();

    let symlink_path = claude_root.join("commands").join("baz.md");
    assert!(symlink_path.is_symlink(), "symlink should exist after install");

    // Figure out the package source identifier for uninstall
    let state: serde_json::Value = serde_json::from_str(
        &fs::read_to_string(jolene_root.path().join("state.json")).unwrap(),
    )
    .unwrap();
    let source = state["packages"][0]["source"].as_str().unwrap().to_string();

    // Uninstall
    jolene_cmd(jolene_root.path(), jolene_home.path())
        .args(["uninstall", &source, "--from", "claude-code"])
        .assert()
        .success();

    assert!(!symlink_path.exists() && !symlink_path.is_symlink(), "symlink should be removed after uninstall");
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
