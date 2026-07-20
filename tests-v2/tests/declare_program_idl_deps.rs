use std::{fs, path::PathBuf, process::Command};

fn write_idl(path: &std::path::Path, constant_name: &str) {
    fs::write(
        path,
        format!(
            r#"{{
  "address": "11111111111111111111111111111111",
  "metadata": {{ "name": "bad", "version": "0.1.0", "spec": "0.1.0" }},
  "instructions": [],
  "constants": [
    {{
      "name": "{constant_name}",
      "type": "u64",
      "value": "1"
    }}
  ]
}}"#
        ),
    )
    .unwrap();
}

fn run_cargo_check(crate_dir: &std::path::Path, target_dir: &std::path::Path) -> std::process::Output {
    Command::new("cargo")
        .args(["check", "--offline", "--manifest-path"])
        .arg(crate_dir.join("Cargo.toml"))
        .env("CARGO_TARGET_DIR", target_dir)
        .output()
        .unwrap_or_else(|err| panic!("failed to run cargo check for {}: {err}", crate_dir.display()))
}

#[test]
#[cfg_attr(
    miri,
    ignore = "spawns cargo and rewrites temporary workspaces; covered by normal cargo test"
)]
fn declare_program_tracks_idl_file_dependencies() {
    let manifest_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let crate_dir = manifest_dir
        .join("target/compile-cases")
        .join("declare_program_idl_dependency_tracking");
    let src_dir = crate_dir.join("src");
    let idl_dir = crate_dir.join("idls");
    let target_dir = crate_dir.join("target");
    let anchor_lang_v2 = manifest_dir
        .parent()
        .expect("tests-v2 should live under the workspace root")
        .join("lang-v2");

    if crate_dir.exists() {
        fs::remove_dir_all(&crate_dir).unwrap();
    }
    fs::create_dir_all(&src_dir).unwrap();
    fs::create_dir_all(&idl_dir).unwrap();

    fs::write(
        crate_dir.join("Cargo.toml"),
        format!(
            r#"[package]
name = "declare_program_idl_dependency_tracking"
version = "0.1.0"
edition = "2021"
publish = false

[dependencies]
anchor-lang-v2 = {{ path = "{}" }}
wincode = {{ version = "0.5", features = ["derive"] }}

[workspace]
"#,
            anchor_lang_v2.display()
        ),
    )
    .unwrap();
    fs::write(
        src_dir.join("lib.rs"),
        r#"
use anchor_lang_v2::prelude::*;

declare_program!(bad);

pub fn read_old_const() -> u64 {
    bad::constants::OLD_CONST
}
"#,
    )
    .unwrap();

    let idl_path = idl_dir.join("bad.json");
    write_idl(&idl_path, "OLD_CONST");

    let first = run_cargo_check(&crate_dir, &target_dir);
    assert!(
        first.status.success(),
        "initial build should succeed\n\nstdout:\n{}\n\nstderr:\n{}",
        String::from_utf8_lossy(&first.stdout),
        String::from_utf8_lossy(&first.stderr),
    );

    write_idl(&idl_path, "NEW_CONST");

    let second = run_cargo_check(&crate_dir, &target_dir);
    assert!(
        !second.status.success(),
        "editing only the imported IDL should invalidate declare_program! output"
    );

    let rendered = format!(
        "{}\n{}",
        String::from_utf8_lossy(&second.stdout),
        String::from_utf8_lossy(&second.stderr),
    );
    assert!(
        rendered.contains("OLD_CONST"),
        "rebuild output should mention the stale constant reference\n\n{rendered}"
    );
}
