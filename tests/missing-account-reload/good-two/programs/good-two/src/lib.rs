use anchor_lang::prelude::*;
use anchor_lang::solana_program::program::invoke;

declare_id!("145kzfkNvXKJx97e3beWGaB4azDNqv6KcjGrvZym6fQb");

#[program]
pub mod good_two {
    use super::*;

    pub fn initialize(ctx: Context<Initialize>) -> Result<()> {
        let user_account = &mut ctx.accounts.user_account;
        user_account.balance = 100;
        user_account.authority = ctx.accounts.authority.key();
        Ok(())
    }

    // GOOD: This function should NOT trigger the linting rule
    pub fn transfer_with_cpi_good(ctx: Context<Transfer>, amount: u64) -> Result<()> {
        let user_account = &mut ctx.accounts.user_account;
        let initial_balance = user_account.balance;

        let instruction = anchor_lang::solana_program::instruction::Instruction {
            program_id: anchor_lang::solana_program::system_program::ID,
            accounts: vec![],
            data: vec![],
        };

        invoke(&instruction, &[])?;

        // GOOD: Reload account to get updated data after CPI
        user_account.reload()?;

        let final_balance = user_account.balance;

        msg!(
            "Initial: {}, Final: {}, Amount: {}",
            initial_balance,
            final_balance,
            amount
        );
        Ok(())
    }

    // GOOD: This function should NOT trigger the linting rule
    pub fn simple_transfer(ctx: Context<Transfer>, amount: u64) -> Result<()> {
        let user_account = &mut ctx.accounts.user_account;
        let balance = user_account.balance;
        user_account.balance = balance - amount;
        Ok(())
    }
}

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
    pub authority: Signer<'info>,
}

#[account]
pub struct UserAccount {
    pub balance: u64,
    pub authority: Pubkey,
}
