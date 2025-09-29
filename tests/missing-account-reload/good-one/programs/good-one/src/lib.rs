use anchor_lang::prelude::*;

declare_id!("145kzfkNvXKJx97e3beWGaB4azDNqv6KcjGrvZym6fQb");

// Macro that performs a CPI using the provided context
macro_rules! do_user_cpi {
    ($ctx:expr) => {{
        use anchor_lang::solana_program::program::invoke;

        let instruction = anchor_lang::solana_program::instruction::Instruction {
            program_id: anchor_lang::solana_program::system_program::ID,
            accounts: vec![],
            data: vec![],
        };

        // CPI call emitted by the macro
        invoke(
            &instruction,
            &[$ctx.accounts.user_account.to_account_info()],
        )?;
    }};
}

#[program]
pub mod good_one {
    use super::*;

    pub fn initialize(ctx: Context<Initialize>) -> Result<()> {
        let user_account = &mut ctx.accounts.user_account;
        user_account.balance = 100;
        user_account.authority = ctx.accounts.authority.key();
        Ok(())
    }

    // Function A: Entry point that calls B
    pub fn transfer_with_cpi_bad(mut ctx: Context<Transfer>, amount: u64) -> Result<()> {
        // Call function B
        function_b(&mut ctx, amount)?;

        // Account usage in A after function call (should trigger warning)
        let balance = ctx.accounts.user_account.balance;
        ctx.accounts.user_account.balance = balance + amount;

        let instruction = anchor_lang::solana_program::instruction::Instruction {
            program_id: anchor_lang::solana_program::system_program::ID,
            accounts: vec![],
            data: vec![],
        };

        // CPI call in function C
        anchor_lang::solana_program::program::invoke(
            &instruction,
            &[ctx.accounts.user_account.to_account_info()],
        )?;

        Ok(())
    }

    // New: Entry point that performs CPI via macro, then uses account without reload
    pub fn transfer_with_macro_cpi_bad(mut ctx: Context<Transfer>, amount: u64) -> Result<()> {
        // CPI via macro expansion
        do_user_cpi!(ctx);

        // Account usage in A after macro CPI (should trigger warning)
        let balance = ctx.accounts.user_account.balance;
        ctx.accounts.user_account.balance = balance + amount;

        Ok(())
    }
}

// Helper functions outside the program module
// Function B: Calls function C and uses account after
fn function_b(ctx: &mut Context<Transfer>, amount: u64) -> Result<()> {
    // Call function C
    function_c(ctx, amount)?;
    ctx.accounts.user_account.reload()?;
    // Account usage in B after function call (should trigger warning)
    let balance = ctx.accounts.user_account.balance;
    function_c(ctx, amount)?;
    ctx.accounts.beneficiary.balance = balance - amount;

    Ok(())
}

// Function C: Performs the actual CPI call
fn function_c(ctx: &mut Context<Transfer>, _amount: u64) -> Result<()> {
    use anchor_lang::solana_program::program::invoke;

    let instruction = anchor_lang::solana_program::instruction::Instruction {
        program_id: anchor_lang::solana_program::system_program::ID,
        accounts: vec![],
        data: vec![],
    };

    // CPI call in function C
    invoke(&instruction, &[ctx.accounts.user_account.to_account_info()])?;

    Ok(())
}

// ...

#[derive(Accounts)]
pub struct Initialize<'info> {
    #[account(init, payer = authority, space = 8 + 8 + 32)]
    pub user_account: Account<'info, UserAccount>,
    #[account(mut)]
    pub authority: Signer<'info>,
    pub system_program: Program<'info, System>,
}

#[derive(Accounts)]
pub struct Transfer<'info> {
    #[account(mut)]
    pub user_account: Account<'info, UserAccount>,
    #[account(mut)]
    pub beneficiary: Account<'info, UserAccount>,
    #[account(mut)]
    pub authority: Signer<'info>,
}

#[account]
pub struct UserAccount {
    pub balance: u64,
    pub authority: Pubkey,
}
