use anchor_lang::prelude::*;
use anchor_lang::solana_program::program::invoke;

declare_id!("145kzfkNvXKJx97e3beWGaB4azDNqv6KcjGrvZym6fQb");

#[program]
pub mod bad_one {
    use super::*;

    pub fn initialize(ctx: Context<Initialize>) -> Result<()> {
        let user_account = &mut ctx.accounts.user_account;
        user_account.balance = 100;
        user_account.authority = ctx.accounts.authority.key();
        Ok(())
    }

    // BAD: This function should trigger the linting rule
    // It accesses user_account.balance after CPI without reloading
    pub fn transfer_with_cpi_bad(ctx: Context<Transfer>, amount: u64) -> Result<()> {
        let user_account = &mut ctx.accounts.user_account;

        // Load initial balance
        let initial_balance = user_account.balance;

        // Create a simple instruction for demonstration
        let instruction = anchor_lang::solana_program::instruction::Instruction {
            program_id: anchor_lang::solana_program::system_program::ID,
            accounts: vec![],
            data: vec![],
        };

        // Perform CPI call
        invoke(&instruction, &[])?;

        // BUG: Accessing account data after CPI without reloading
        let final_balance = user_account.balance;

        msg!("Initial balance: {}", initial_balance);
        msg!("Final balance: {}", final_balance);
        msg!("Amount: {}", amount);

        Ok(())
    }

    // BAD: This function should trigger the linting rule
    // It accesses account data after multiple CPI calls without reloading
    pub fn transfer_with_multiple_cpi_bad(ctx: Context<Transfer>, amount: u64) -> Result<()> {
        let user_account = &mut ctx.accounts.user_account;

        // Load initial balance
        let initial_balance = user_account.balance;

        // Create instructions for demonstration
        let instruction1 = anchor_lang::solana_program::instruction::Instruction {
            program_id: anchor_lang::solana_program::system_program::ID,
            accounts: vec![],
            data: vec![],
        };

        let instruction2 = anchor_lang::solana_program::instruction::Instruction {
            program_id: anchor_lang::solana_program::system_program::ID,
            accounts: vec![],
            data: vec![],
        };

        // Perform multiple CPI calls
        invoke(&instruction1, &[])?;
        invoke(&instruction2, &[])?;

        // BUG: Accessing account data after CPI without reloading
        let final_balance = user_account.balance;

        msg!("Initial balance: {}", initial_balance);
        msg!("Final balance: {}", final_balance);
        msg!("Amount: {}", amount);

        Ok(())
    }

    // GOOD: This function should NOT trigger the linting rule
    // It doesn't use CPI at all
    pub fn simple_transfer(ctx: Context<Transfer>, amount: u64) -> Result<()> {
        let user_account = &mut ctx.accounts.user_account;

        // Simple account access without CPI - this is fine
        let balance = user_account.balance;
        user_account.balance = balance - amount;

        Ok(())
    }
}

#[derive(Accounts)]
pub struct Initialize<'info> {
    #[account(
        init,
        payer = authority,
        space = 8 + 8 + 32
    )]
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
