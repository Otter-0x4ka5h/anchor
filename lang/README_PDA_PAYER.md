# PDA Payer Support in Anchor

This feature allows Anchor to automatically detect when a payer account has PDA constraints (seeds and bump) and generate the appropriate `invoke_signed` calls for PDA signing.

## How It Works

Instead of creating new constraint types, Anchor automatically detects when a `payer` constraint points to an account with PDA constraints and handles the signing automatically.

## Usage

```rust
#[derive(Accounts)]
pub struct Initialize<'info> {
    #[account(
        init,
        payer = pda_payer,  // Anchor detects this is a PDA
        space = 8 + 8 + 32,
        seeds = ["my_account"],
        bump
    )]
    pub my_account: Account<'info, MyData>,
    
    // This account has PDA constraints, so Anchor automatically
    // uses invoke_signed when it's used as a payer
    #[account(
        seeds = ["payer"],
        bump,
        // Must be system program owned for simplicity
    )]
    pub pda_payer: AccountInfo<'info>,
    
    pub system_program: Program<'info, System>,
}
```

## Implementation Details

1. **Automatic Detection**: When parsing constraints, Anchor checks if the payer target has `seeds` and `bump` constraints
2. **Code Generation**: If PDA constraints are detected, Anchor generates:
   - `find_program_address` call for the payer
   - PDA signing code for all System Program CPIs
3. **Fallback**: If no PDA constraints are detected, normal signing is used

## Benefits

- ✅ **Simple syntax** - just use existing `payer` constraint
- ✅ **Automatic detection** - no need to specify PDA behavior
- ✅ **Backward compatible** - existing code continues to work
- ✅ **Less code** - no new constraint types needed

## Limitations

- Currently only supports **system-program-owned PDAs** for simplicity
- **Program-owned PDA payers** would require more complex implementation
- Requires the payer account to have both `seeds` and `bump` constraints

## Future Enhancements

- Support for program-owned PDA payers
- Support for composite PDA constraints
- Better error messages for invalid PDA configurations
