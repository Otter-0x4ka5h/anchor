use {
    ::anchor_lang as anchor_lang_crate,
    anchor_lang_crate::{
        err, require, require_eq, require_gt, require_gte, require_keys_eq, require_keys_neq,
        require_neq, solana_program::pubkey::Pubkey,
    },
};

#[allow(dead_code)]
#[derive(Clone, Copy)]
struct Hijacked;

#[allow(dead_code)]
impl Hijacked {
    fn with_values<T>(self, _values: T) -> Self {
        self
    }

    fn with_pubkeys<T>(self, _pubkeys: T) -> Self {
        self
    }
}

#[allow(unused_macros)]
macro_rules! error {
    ($($tt:tt)*) => {
        Hijacked
    };
}

#[allow(dead_code, non_snake_case)]
fn Err<T>(_value: T) -> anchor_lang_crate::Result<()> {
    Ok(())
}

#[allow(dead_code, unused_imports)]
mod anchor_lang {
    pub use crate::shadowed_error as error;
}

#[allow(unused_macros)]
#[macro_export]
macro_rules! shadowed_error {
    ($($tt:tt)*) => {
        $crate::Hijacked
    };
}

fn err_tt_arm() -> anchor_lang_crate::Result<()> {
    err!(RequireViolated)
}

fn err_expr_arm() -> anchor_lang_crate::Result<()> {
    err!(anchor_lang_crate::error::ErrorCode::RequireViolated)
}

fn assert_error(result: anchor_lang_crate::Result<()>, code: anchor_lang_crate::error::ErrorCode) {
    assert_eq!(result, ::core::result::Result::Err(code.into()));
}

#[test]
fn require_and_err_ignore_shadowed_callers() {
    assert_error(
        (|| -> anchor_lang_crate::Result<()> {
            require!(false, RequireViolated);
            Ok(())
        })(),
        anchor_lang_crate::error::ErrorCode::RequireViolated,
    );

    assert_error(
        (|| -> anchor_lang_crate::Result<()> {
            require!(false, anchor_lang_crate::error::ErrorCode::RequireViolated);
            Ok(())
        })(),
        anchor_lang_crate::error::ErrorCode::RequireViolated,
    );

    assert_error(
        err_tt_arm(),
        anchor_lang_crate::error::ErrorCode::RequireViolated,
    );

    assert_error(
        err_expr_arm(),
        anchor_lang_crate::error::ErrorCode::RequireViolated,
    );
}

#[test]
fn comparison_macros_ignore_shadowed_callers() {
    assert_error(
        (|| -> anchor_lang_crate::Result<()> {
            require_eq!(1, 2);
            Ok(())
        })(),
        anchor_lang_crate::error::ErrorCode::RequireEqViolated,
    );

    assert_error(
        (|| -> anchor_lang_crate::Result<()> {
            require_eq!(1, 2, anchor_lang_crate::error::ErrorCode::RequireViolated);
            Ok(())
        })(),
        anchor_lang_crate::error::ErrorCode::RequireViolated,
    );

    assert_error(
        (|| -> anchor_lang_crate::Result<()> {
            require_neq!(1, 1);
            Ok(())
        })(),
        anchor_lang_crate::error::ErrorCode::RequireNeqViolated,
    );

    assert_error(
        (|| -> anchor_lang_crate::Result<()> {
            require_neq!(1, 1, anchor_lang_crate::error::ErrorCode::RequireViolated);
            Ok(())
        })(),
        anchor_lang_crate::error::ErrorCode::RequireViolated,
    );

    assert_error(
        (|| -> anchor_lang_crate::Result<()> {
            let left = Pubkey::new_unique();
            let right = Pubkey::new_unique();
            require_keys_eq!(left, right);
            Ok(())
        })(),
        anchor_lang_crate::error::ErrorCode::RequireKeysEqViolated,
    );

    assert_error(
        (|| -> anchor_lang_crate::Result<()> {
            let left = Pubkey::new_unique();
            let right = Pubkey::new_unique();
            require_keys_eq!(
                left,
                right,
                anchor_lang_crate::error::ErrorCode::RequireViolated
            );
            Ok(())
        })(),
        anchor_lang_crate::error::ErrorCode::RequireViolated,
    );

    assert_error(
        (|| -> anchor_lang_crate::Result<()> {
            let key = Pubkey::new_unique();
            require_keys_neq!(key, key);
            Ok(())
        })(),
        anchor_lang_crate::error::ErrorCode::RequireKeysNeqViolated,
    );

    assert_error(
        (|| -> anchor_lang_crate::Result<()> {
            let key = Pubkey::new_unique();
            require_keys_neq!(
                key,
                key,
                anchor_lang_crate::error::ErrorCode::RequireViolated
            );
            Ok(())
        })(),
        anchor_lang_crate::error::ErrorCode::RequireViolated,
    );

    assert_error(
        (|| -> anchor_lang_crate::Result<()> {
            require_gt!(1, 2);
            Ok(())
        })(),
        anchor_lang_crate::error::ErrorCode::RequireGtViolated,
    );

    assert_error(
        (|| -> anchor_lang_crate::Result<()> {
            require_gt!(1, 2, anchor_lang_crate::error::ErrorCode::RequireViolated);
            Ok(())
        })(),
        anchor_lang_crate::error::ErrorCode::RequireViolated,
    );

    assert_error(
        (|| -> anchor_lang_crate::Result<()> {
            require_gte!(1, 2);
            Ok(())
        })(),
        anchor_lang_crate::error::ErrorCode::RequireGteViolated,
    );

    assert_error(
        (|| -> anchor_lang_crate::Result<()> {
            require_gte!(1, 2, anchor_lang_crate::error::ErrorCode::RequireViolated);
            Ok(())
        })(),
        anchor_lang_crate::error::ErrorCode::RequireViolated,
    );
}
