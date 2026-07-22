use std::{fs, path::PathBuf, process::Command};

fn compile_pass_case(name: &str, source: &str) {
    let manifest_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let crate_dir = manifest_dir.join("target/compile-cases").join(name);
    let src_dir = crate_dir.join("src");
    let anchor_lang_v2 = manifest_dir
        .parent()
        .expect("tests-v2 should live under the workspace root")
        .join("lang-v2");

    if crate_dir.exists() {
        fs::remove_dir_all(&crate_dir).unwrap();
    }
    fs::create_dir_all(&src_dir).unwrap();

    fs::write(
        crate_dir.join("Cargo.toml"),
        format!(
            r#"[package]
name = "{name}"
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
    fs::write(src_dir.join("lib.rs"), source).unwrap();

    let output = Command::new("cargo")
        .args(["check", "--offline", "--manifest-path"])
        .arg(crate_dir.join("Cargo.toml"))
        .output()
        .unwrap_or_else(|err| panic!("failed to run cargo check for {name}: {err}"));

    assert!(
        output.status.success(),
        "{name} failed to compile\n\nstdout:\n{}\n\nstderr:\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr),
    );
}

#[test]
#[cfg_attr(
    miri,
    ignore = "spawns cargo and writes temporary workspaces; covered by normal cargo test"
)]
fn nontrivial_as_ref_seed_receivers_compile() {
    compile_pass_case(
        "tests_v2_seed_nontrivial_as_ref",
        r#"
use anchor_lang_v2::prelude::*;

declare_id!("11111111111111111111111111111111");

#[derive(anchor_lang_v2::wincode::SchemaRead, anchor_lang_v2::wincode::SchemaWrite)]
pub struct SeedBuf(Vec<u8>);

impl SeedBuf {
    pub fn as_ref(&self) -> &[u8] {
        self.0.as_slice()
    }
}

#[derive(anchor_lang_v2::wincode::SchemaRead, anchor_lang_v2::wincode::SchemaWrite)]
pub struct SeedConfig {
    pub seed: SeedBuf,
}

impl Owner for SeedConfig {
    const OWNER: Address = crate::ID;
}

impl Discriminator for SeedConfig {
    const DISCRIMINATOR: &'static [u8] = &[0x63, 0x66, 0x67, 0x2d, 0x73, 0x65, 0x65, 0x64];
}

#[derive(Accounts)]
pub struct Good {
    pub config: BorshAccount<SeedConfig>,
    #[account(seeds = [config.seed.as_ref()], bump)]
    pub target: UncheckedAccount,
}
"#,
    );
}

#[test]
#[cfg_attr(
    miri,
    ignore = "spawns cargo and writes temporary workspaces; covered by normal cargo test"
)]
fn opaque_init_seed_expressions_keep_bump_bytes_alive() {
    compile_pass_case(
        "tests_v2_init_opaque_seed_expr",
        r#"
use anchor_lang_v2::prelude::*;

declare_id!("11111111111111111111111111111111");

#[derive(anchor_lang_v2::wincode::SchemaRead, anchor_lang_v2::wincode::SchemaWrite)]
pub struct Data {
    pub value: u64,
}

impl Owner for Data {
    const OWNER: Address = crate::ID;
}

impl Discriminator for Data {
    const DISCRIMINATOR: &'static [u8] = &[0x64, 0x61, 0x74, 0x61, 0x2d, 0x62, 0x6f, 0x72];
}

pub struct SeedBundle<'a>([&'a [u8]; 1]);

impl<'a> SeedBundle<'a> {
    pub fn for_payer(payer: &'a [u8]) -> Self {
        Self([payer])
    }
}

impl<'a> AsRef<[&'a [u8]]> for SeedBundle<'a> {
    fn as_ref(&self) -> &[&'a [u8]] {
        &self.0
    }
}

#[derive(Accounts)]
pub struct Good {
    #[account(mut)]
    pub payer: Signer,
    #[account(
        init,
        payer = payer,
        space = 8 + core::mem::size_of::<Data>(),
        seeds = SeedBundle::for_payer(payer.address().as_ref()),
        bump
    )]
    pub data: BorshAccount<Data>,
    pub system_program: Program<System>,
}
"#,
    );
}
