use std::{fs, path::PathBuf, process::Command};

fn compile_fail_case(name: &str, source: &str, snippets: &[&str]) {
    let manifest_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let crate_dir = manifest_dir.join("target/macro-diagnostics").join(name);
    let src_dir = crate_dir.join("src");
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
anchor-spl-v2 = {{ path = "{}" }}
wincode = {{ version = "0.5", features = ["derive"] }}

[workspace]
"#,
            manifest_dir.display(),
            manifest_dir.parent().unwrap().join("spl-v2").display()
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
        !output.status.success(),
        "{name} unexpectedly compiled successfully"
    );
    let stderr = String::from_utf8_lossy(&output.stderr);
    for snippet in snippets {
        assert!(
            stderr.contains(snippet),
            "{name} stderr did not contain {snippet:?}\n\nstderr:\n{stderr}"
        );
    }
}

#[test]
#[cfg_attr(
    miri,
    ignore = "spawns cargo and writes temporary workspaces; covered by normal cargo test"
)]
fn raw_constraint_rejects_obvious_non_bool_literals() {
    compile_fail_case(
        "raw_constraint_non_bool",
        r#"
use anchor_lang_v2::prelude::*;

#[derive(Accounts)]
pub struct Bad {
    #[account(constraint = "hello")]
    pub data: UncheckedAccount,
}
"#,
        &[
            "`constraint` expects a boolean expression",
            "non-boolean literals",
        ],
    );
}

#[test]
#[cfg_attr(
    miri,
    ignore = "spawns cargo and writes temporary workspaces; covered by normal cargo test"
)]
fn invalid_account_arguments_are_targeted() {
    compile_fail_case(
        "invalid_account_argument",
        r#"
use anchor_lang_v2::prelude::*;

#[derive(Accounts)]
pub struct Bad {
    #[account(singler)]
    pub data: UncheckedAccount,
}
"#,
        &["unknown account constraint `singler`"],
    );
}

#[test]
#[cfg_attr(
    miri,
    ignore = "spawns cargo and writes temporary workspaces; covered by normal cargo test"
)]
fn unsafe_dup_constraint_has_targeted_message() {
    compile_fail_case(
        "unsafe_dup_required",
        r#"
use anchor_lang_v2::prelude::*;

#[derive(Accounts)]
pub struct Bad {
    #[account(dup)]
    pub data: UncheckedAccount,
}
"#,
        &[
            "`dup` bypasses duplicate-account safety checks",
            "unsafe(dup)",
        ],
    );
}

#[test]
#[cfg_attr(
    miri,
    ignore = "spawns cargo and writes temporary workspaces; covered by normal cargo test"
)]
fn init_space_rejects_union_and_unsized_reference_fields() {
    compile_fail_case(
        "init_space_union",
        r#"
use anchor_lang_v2::InitSpace;

#[derive(Copy, Clone, InitSpace)]
union Bad {
    value: u64,
}
"#,
        &["#[derive(InitSpace)] only supports structs and enums"],
    );

    compile_fail_case(
        "init_space_reference",
        r#"
use anchor_lang_v2::InitSpace;

#[derive(InitSpace)]
pub struct Bad<'a> {
    pub name: &'a str,
}
"#,
        &[
            "#[derive(InitSpace)] can't compute size for this type",
            "fixed-size alternative",
        ],
    );
}

#[test]
#[cfg_attr(
    miri,
    ignore = "spawns cargo and writes temporary workspaces; covered by normal cargo test"
)]
fn malformed_discriminator_attribute_has_targeted_message() {
    compile_fail_case(
        "bad_discriminator",
        r#"
use anchor_lang_v2::prelude::*;

declare_id!("11111111111111111111111111111111");

#[program]
pub mod bad_discriminator {
    use super::*;

    #[discrim = "bad"]
    pub fn ix(_ctx: &mut Context<Noop>) -> Result<()> {
        Ok(())
    }
}

#[derive(Accounts)]
pub struct Noop {}
"#,
        &["`#[discrim = ...]` value must be an integer literal or byte array literal"],
    );
}

#[test]
#[cfg_attr(
    miri,
    ignore = "spawns cargo and writes temporary workspaces; covered by normal cargo test"
)]
fn instruction_args_must_match_zero_arg_handler() {
    compile_fail_case(
        "instruction_args_without_handler_args",
        r#"
use anchor_lang_v2::prelude::*;

declare_id!("11111111111111111111111111111111");

#[program]
pub mod instruction_args_without_handler_args {
    use super::*;

    pub fn ix(ctx: &mut Context<Bad>) -> Result<()> {
        let _ = ctx;
        Ok(())
    }
}

#[derive(Accounts)]
#[instruction(value: u64)]
pub struct Bad {
    #[account(constraint = value > 0)]
    pub data: UncheckedAccount,
}
"#,
        &["expected `()`, found `(u64,)`"],
    );
}

#[test]
#[cfg_attr(
    miri,
    ignore = "spawns cargo and writes temporary workspaces; covered by normal cargo test"
)]
fn slot_hashes_is_not_a_supported_sysvar_account() {
    compile_fail_case(
        "unsupported_slot_hashes_sysvar",
        r#"
use anchor_lang_v2::{accounts::Sysvar, pinocchio, AnchorAccount};

type SlotHashes = pinocchio::sysvars::slot_hashes::SlotHashes<&'static [u8]>;

fn assert_anchor_account<T: AnchorAccount>() {}

fn check() {
    assert_anchor_account::<Sysvar<SlotHashes>>();
}
"#,
        &["SlotHashes", "SysvarId"],
    );
}

#[test]
#[cfg_attr(
    miri,
    ignore = "spawns cargo and writes temporary workspaces; covered by normal cargo test"
)]
fn init_payer_must_be_mutable() {
    compile_fail_case(
        "init_payer_must_be_mutable",
        r#"
use anchor_lang_v2::prelude::*;

declare_id!("11111111111111111111111111111111");

#[account]
pub struct Data {
    pub value: u64,
}

#[derive(Accounts)]
pub struct Bad {
    #[account(init, payer = payer, space = 8 + core::mem::size_of::<Data>())]
    pub data: Account<Data>,
    pub payer: Signer,
    pub system_program: Program<System>,
}
"#,
        &["the payer specified for an init constraint must be mutable"],
    );
}

#[test]
#[cfg_attr(
    miri,
    ignore = "spawns cargo and writes temporary workspaces; covered by normal cargo test"
)]
fn pda_init_payer_must_be_system_account() {
    compile_fail_case(
        "pda_init_payer_must_be_system_account",
        r#"
use anchor_lang_v2::prelude::*;

#[account]
pub struct Data {
    pub value: u64,
}

#[derive(Accounts)]
pub struct Bad {
    #[account(mut, seeds = [b"payer"], bump)]
    pub payer: UncheckedAccount,
    #[account(init, payer = payer, space = 8 + core::mem::size_of::<Data>())]
    pub data: Account<Data>,
    pub system_program: Program<System>,
}
"#,
        &["PDA init payers must be declared as `SystemAccount`"],
    );
}

#[test]
#[cfg_attr(
    miri,
    ignore = "spawns cargo and writes temporary workspaces; covered by normal cargo test"
)]
fn optional_pda_init_payer_is_rejected() {
    compile_fail_case(
        "optional_pda_init_payer_is_rejected",
        r#"
use anchor_lang_v2::prelude::*;

#[account]
pub struct Data {
    pub value: u64,
}

#[derive(Accounts)]
pub struct Bad {
    #[account(mut, seeds = [b"payer"], bump)]
    pub payer: Option<SystemAccount>,
    #[account(init, payer = payer, space = 8 + core::mem::size_of::<Data>())]
    pub data: Account<Data>,
    pub system_program: Program<System>,
}
"#,
        &["optional accounts cannot be used as init payers"],
    );
}

#[test]
#[cfg_attr(
    miri,
    ignore = "spawns cargo and writes temporary workspaces; covered by normal cargo test"
)]
fn account_attrs_reject_duplicate_singletons_and_conflicts() {
    compile_fail_case(
        "duplicate_mut_across_account_attrs",
        r#"
use anchor_lang_v2::prelude::*;

#[derive(Accounts)]
pub struct Bad {
    #[account(mut)]
    #[account(mut)]
    pub data: UncheckedAccount,
}
"#,
        &["`mut` already provided"],
    );

    compile_fail_case(
        "duplicate_signer_constraint",
        r#"
use anchor_lang_v2::prelude::*;

#[derive(Accounts)]
pub struct Bad {
    #[account(signer, signer)]
    pub data: UncheckedAccount,
}
"#,
        &["`signer` already provided"],
    );

    compile_fail_case(
        "duplicate_executable_constraint",
        r#"
use anchor_lang_v2::prelude::*;

#[derive(Accounts)]
pub struct Bad {
    #[account(executable)]
    #[account(executable)]
    pub program: UncheckedAccount,
}
"#,
        &["`executable` already provided"],
    );

    compile_fail_case(
        "duplicate_unsafe_dup_constraint",
        r#"
use anchor_lang_v2::prelude::*;

#[derive(Accounts)]
pub struct Bad {
    #[account(unsafe(dup))]
    #[account(unsafe(dup))]
    pub data: UncheckedAccount,
}
"#,
        &["`unsafe(dup)` already provided"],
    );

    compile_fail_case(
        "duplicate_realloc_zero_constraint",
        r#"
use anchor_lang_v2::prelude::*;

#[derive(Accounts)]
pub struct Bad {
    #[account(mut, realloc = 16, realloc_payer = payer, realloc_zero = false, realloc_zero = false)]
    pub data: BorshAccount<Inner>,
    #[account(mut)]
    pub payer: Signer,
}

#[derive(wincode::SchemaRead, wincode::SchemaWrite, Default)]
pub struct Inner {
    pub value: u64,
}
"#,
        &["`realloc_zero` already provided"],
    );

    compile_fail_case(
        "duplicate_owner_across_account_attrs",
        r#"
use anchor_lang_v2::prelude::*;

declare_id!("11111111111111111111111111111111");

#[derive(Accounts)]
pub struct Bad {
    #[account(owner = System::id())]
    #[account(owner = System::id())]
    pub data: UncheckedAccount,
}
"#,
        &["`owner` already provided"],
    );

    compile_fail_case(
        "duplicate_has_one_target",
        r#"
use anchor_lang_v2::prelude::*;

#[account]
pub struct Data {
    pub authority: Address,
}

#[derive(Accounts)]
pub struct Bad {
    pub authority: UncheckedAccount,
    #[account(has_one = authority, has_one = authority)]
    pub data: Account<Data>,
}
"#,
        &["duplicate `has_one = authority` constraint"],
    );

    compile_fail_case(
        "init_and_zeroed_conflict",
        r#"
use anchor_lang_v2::prelude::*;

#[account]
pub struct Data {
    pub value: u64,
}

#[derive(Accounts)]
pub struct Bad {
    #[account(mut)]
    pub payer: Signer,
    #[account(init, zeroed, payer = payer, space = 8 + core::mem::size_of::<Data>())]
    pub data: Account<Data>,
    pub system_program: Program<System>,
}
"#,
        &["only one of `init`, `init_if_needed`, and `zeroed` may be used"],
    );

    compile_fail_case(
        "init_and_close_conflict",
        r#"
use anchor_lang_v2::prelude::*;

#[account]
pub struct Data {
    pub value: u64,
}

#[derive(Accounts)]
pub struct Bad {
    #[account(mut)]
    pub payer: Signer,
    #[account(mut)]
    pub receiver: SystemAccount,
    #[account(
        init,
        payer = payer,
        space = 8 + core::mem::size_of::<Data>(),
        close = receiver,
    )]
    pub data: Account<Data>,
    pub system_program: Program<System>,
}
"#,
        &["`close` cannot be used with `init`, `init_if_needed`, or `zeroed`"],
    );

    compile_fail_case(
        "init_if_needed_and_realloc_conflict",
        r#"
use anchor_lang_v2::prelude::*;

#[account]
pub struct Data {
    pub value: u64,
}

#[derive(Accounts)]
pub struct Bad {
    #[account(mut)]
    pub payer: Signer,
    #[account(
        init_if_needed,
        payer = payer,
        space = 8 + core::mem::size_of::<Data>(),
        realloc = 64,
        realloc_payer = payer,
        realloc_zero = false,
    )]
    pub data: Account<Data>,
    pub system_program: Program<System>,
}
"#,
        &["`realloc` cannot be used with `init`, `init_if_needed`, or `zeroed`"],
    );
}

#[test]
#[cfg_attr(
    miri,
    ignore = "spawns cargo and writes temporary workspaces; covered by normal cargo test"
)]
fn account_attrs_enforce_local_dependency_pairs() {
    compile_fail_case(
        "payer_without_init",
        r#"
use anchor_lang_v2::prelude::*;

#[derive(Accounts)]
pub struct Bad {
    #[account(payer = payer)]
    pub data: UncheckedAccount,
    #[account(mut)]
    pub payer: Signer,
}
"#,
        &["`payer` requires `init` or `init_if_needed`"],
    );

    compile_fail_case(
        "space_without_init",
        r#"
use anchor_lang_v2::prelude::*;

#[derive(Accounts)]
pub struct Bad {
    #[account(space = 16)]
    pub data: UncheckedAccount,
}
"#,
        &["`space` requires `init` or `init_if_needed`"],
    );

    compile_fail_case(
        "bump_without_seeds",
        r#"
use anchor_lang_v2::prelude::*;

#[derive(Accounts)]
pub struct Bad {
    #[account(bump)]
    pub data: UncheckedAccount,
}
"#,
        &["`bump` requires `seeds`"],
    );

    compile_fail_case(
        "seeds_without_bump",
        r#"
use anchor_lang_v2::prelude::*;

#[derive(Accounts)]
pub struct Bad {
    #[account(seeds = [b"data"])]
    pub data: UncheckedAccount,
}
"#,
        &["`seeds` requires `bump`"],
    );

    compile_fail_case(
        "seeds_program_without_seeds",
        r#"
use anchor_lang_v2::prelude::*;

declare_id!("11111111111111111111111111111111");

pub const OTHER_PROGRAM: Address =
    Address::from_str_const("Gue5TpR6sstSyGhSvmVeH2TeKqBYYqmXpRCacB9jAk8u");

#[derive(Accounts)]
pub struct Bad {
    #[account(seeds::program = OTHER_PROGRAM)]
    pub data: UncheckedAccount,
}
"#,
        &["`seeds::program` requires `seeds`"],
    );

    compile_fail_case(
        "realloc_payer_without_realloc",
        r#"
use anchor_lang_v2::prelude::*;

#[derive(Accounts)]
pub struct Bad {
    #[account(realloc_payer = payer)]
    pub data: UncheckedAccount,
    #[account(mut)]
    pub payer: Signer,
}
"#,
        &["`realloc_payer` requires `realloc`"],
    );
}

#[test]
#[cfg_attr(
    miri,
    ignore = "spawns cargo and writes temporary workspaces; covered by normal cargo test"
)]
fn spl_init_constraints_require_pairs() {
    compile_fail_case(
        "init_token_requires_authority_with_mint",
        r#"
use anchor_lang_v2::prelude::*;
use anchor_spl_v2::{
    mint::{self},
    token::{self},
    token_interface::{Mint, TokenAccount, TokenInterface},
};

#[derive(Accounts)]
pub struct Bad {
    #[account(mut)]
    pub payer: Signer,
    pub mint: InterfaceAccount<Mint>,
    pub token_program: Interface<'static, TokenInterface>,
    #[account(init, payer = payer, token::mint = mint)]
    pub token_account: InterfaceAccount<TokenAccount>,
    pub system_program: Program<System>,
}
"#,
        &["when initializing, `token::authority` must be provided if `token::mint` is"],
    );

    compile_fail_case(
        "init_token_requires_mint_with_authority",
        r#"
use anchor_lang_v2::prelude::*;
use anchor_spl_v2::{
    mint::{self},
    token::{self},
    token_interface::{TokenAccount, TokenInterface},
};

#[derive(Accounts)]
pub struct Bad {
    #[account(mut)]
    pub payer: Signer,
    pub authority: UncheckedAccount,
    pub token_program: Interface<'static, TokenInterface>,
    #[account(init, payer = payer, token::authority = authority)]
    pub token_account: InterfaceAccount<TokenAccount>,
    pub system_program: Program<System>,
}
"#,
        &["when initializing, `token::mint` must be provided if `token::authority` is"],
    );

    compile_fail_case(
        "init_mint_requires_authority_with_decimals",
        r#"
use anchor_lang_v2::prelude::*;
use anchor_spl_v2::{
    mint::{self},
    token::{self},
    token_interface::Mint,
};

#[derive(Accounts)]
pub struct Bad {
    #[account(mut)]
    pub payer: Signer,
    #[account(init, payer = payer, mint::decimals = 6)]
    pub mint: InterfaceAccount<Mint>,
    pub system_program: Program<System>,
}
"#,
        &["when initializing, `mint::authority` must be provided if `mint::decimals` is"],
    );

    compile_fail_case(
        "init_mint_requires_decimals_with_authority",
        r#"
use anchor_lang_v2::prelude::*;
use anchor_spl_v2::{
    mint::{self},
    token::{self},
    token_interface::Mint,
};

#[derive(Accounts)]
pub struct Bad {
    #[account(mut)]
    pub payer: Signer,
    pub authority: UncheckedAccount,
    #[account(init, payer = payer, mint::authority = authority)]
    pub mint: InterfaceAccount<Mint>,
    pub system_program: Program<System>,
}
"#,
        &["when initializing, `mint::decimals` must be provided if `mint::authority` is"],
    );
}

#[test]
#[cfg_attr(
    miri,
    ignore = "spawns cargo and writes temporary workspaces; covered by normal cargo test"
)]
fn init_owner_override_rejects_typed_accounts() {
    fn case(name: &str, account_attr: &str, field_ty: &str) {
        let source = format!(
            r#"
use anchor_lang_v2::prelude::*;

declare_id!("11111111111111111111111111111111");

pub const OTHER_PROGRAM: Address =
    Address::from_str_const("Gue5TpR6sstSyGhSvmVeH2TeKqBYYqmXpRCacB9jAk8u");

{account_attr}
pub struct Data {{
    pub value: u64,
}}

#[derive(Accounts)]
pub struct Bad {{
    #[account(mut)]
    pub payer: Signer,
    #[account(
        init,
        payer = payer,
        space = 8 + core::mem::size_of::<Data>(),
        owner = OTHER_PROGRAM,
    )]
    pub data: {field_ty},
    pub system_program: Program<System>,
}}
"#
        );
        compile_fail_case(name, &source, &["ForeignOwnerInit", "is not implemented"]);
    }

    case("init_owner_override_account", "#[account]", "Account<Data>");
    case(
        "init_owner_override_boxed_account",
        "#[account]",
        "Box<Account<Data>>",
    );
    case(
        "init_owner_override_interface_account",
        "#[account]",
        "InterfaceAccount<Data>",
    );
    case(
        "init_owner_override_borsh_account",
        "#[account(borsh)]",
        "BorshAccount<Data>",
    );
    case(
        "init_owner_override_boxed_borsh_account",
        "#[account(borsh)]",
        "Box<BorshAccount<Data>>",
    );
}
