use crate::states::*;
use anchor_lang::prelude::*;
use anchor_spl::{
    token::{self, Token},
    token_2022::{
        self,
        spl_token_2022::{
            self,
            extension::{
                transfer_fee::{TransferFeeConfig, MAX_FEE_BASIS_POINTS},
                BaseStateWithExtensions, ExtensionType, StateWithExtensions,
            },
        },
    },
    token_interface::{Mint, TokenAccount},
};
use std::collections::HashSet;

use super::get_recent_epoch;

const MINT_WHITELIST: [&'static str; 4] = [
    "HVbpJAQGNpkgBaYBZQBR1t7yFdvaYVp2vCQQfKKEN4tM",
    "Crn4x1Y2HUKko7ox2EZMT6N2t2ZyH7eKtwkBGVnhEq1g",
    "FrBfWJ4qE5sCzKm3k3JaAtqZcXUh4LvJygDeketsrsH4",
    "2b1kV6DkPAnxd5ixfnxCpjxmKwqjjaYmCZfHsFu24GXo",
];

pub fn invoke_memo_instruction<'info>(
    memo_msg: &[u8],                  // The message to be attached as a memo.
    memo_program: AccountInfo<'info>, // Account info for the Memo program.
) -> solana_program::entrypoint::ProgramResult {
    // Step 1: Build the memo instruction.
    let ix = spl_memo::build_memo(memo_msg, &Vec::new());

    // Step 2: Create a list of accounts involved in the instruction.
    let accounts = vec![memo_program];

    // Step 3: Invoke the memo instruction using the Solana runtime.
    solana_program::program::invoke(&ix, &accounts[..])
}

pub fn transfer_from_user_to_pool_vault<'info>(
    signer: &Signer<'info>, // The user authorizing the transfer.
    from: &InterfaceAccount<'info, TokenAccount>, // The source account (user's token account).
    to_vault: &InterfaceAccount<'info, TokenAccount>, // The target pool vault (destination).
    mint: Option<Box<InterfaceAccount<'info, Mint>>>, // Optional mint account info for Token-2022 transfers.
    token_program: &AccountInfo<'info>,               // The SPL token program account.
    token_program_2022: Option<AccountInfo<'info>>,   // Optional SPL Token-2022 program account.
    amount: u64,                                      // Amount to transfer.
) -> Result<()> {
    if amount == 0 {
        return Ok(());
    }
    let mut token_program_info = token_program.to_account_info();
    let from_token_info = from.to_account_info();
    match (mint, token_program_2022) {
        // Case 1: Use Token-2022 for transfer if both mint and token_program_2022 are provided.
        (Some(mint), Some(token_program_2022)) => {
            if from_token_info.owner == token_program_2022.key {
                token_program_info = token_program_2022.to_account_info()
            }

            // Token-2022 transfer with checked transfer (ensuring decimals).
            token_2022::transfer_checked(
                CpiContext::new(
                    token_program_info,
                    token_2022::TransferChecked {
                        from: from_token_info,
                        to: to_vault.to_account_info(),
                        authority: signer.to_account_info(),
                        mint: mint.to_account_info(),
                    },
                ),
                amount,
                mint.decimals,
            )
        }
        // Case 2: Use standard SPL token transfer.
        _ => token::transfer(
            CpiContext::new(
                token_program_info,
                token::Transfer {
                    from: from_token_info,
                    to: to_vault.to_account_info(),
                    authority: signer.to_account_info(),
                },
            ),
            amount,
        ),
    }
}

pub fn transfer_from_pool_vault_to_user<'info>(
    pool_state_loader: &AccountLoader<'info, PoolState>, // The pool state loader containing authority info.
    from_vault: &InterfaceAccount<'info, TokenAccount>, // The vault account from which tokens are transferred.
    to: &InterfaceAccount<'info, TokenAccount>,         // The user's account where tokens are sent.
    mint: Option<Box<InterfaceAccount<'info, Mint>>>, // Optional mint account info for Token-2022 transfers.
    token_program: &AccountInfo<'info>,               // The SPL token program account.
    token_program_2022: Option<AccountInfo<'info>>,   // Optional SPL Token-2022 program account.
    amount: u64,                                      // Amount to transfer.
) -> Result<()> {
    if amount == 0 {
        return Ok(());
    }
    let mut token_program_info = token_program.to_account_info();
    let from_vault_info = from_vault.to_account_info();
    match (mint, token_program_2022) {
        // Case 1: Token-2022 transfer if both mint and token_program_2022 are provided.
        (Some(mint), Some(token_program_2022)) => {
            if from_vault_info.owner == token_program_2022.key {
                token_program_info = token_program_2022.to_account_info()
            }

            // Token-2022 transfer with checked transfer (ensuring decimals).
            token_2022::transfer_checked(
                CpiContext::new_with_signer(
                    token_program_info,
                    token_2022::TransferChecked {
                        from: from_vault_info,
                        to: to.to_account_info(),
                        authority: pool_state_loader.to_account_info(),
                        mint: mint.to_account_info(),
                    },
                    &[&pool_state_loader.load()?.seeds()],
                ),
                amount,
                mint.decimals,
            )
        }
        // Case 2: Standard SPL token transfer.
        _ => token::transfer(
            CpiContext::new_with_signer(
                token_program_info,
                token::Transfer {
                    from: from_vault_info,
                    to: to.to_account_info(),
                    authority: pool_state_loader.to_account_info(),
                },
                &[&pool_state_loader.load()?.seeds()],
            ),
            amount,
        ),
    }
}

pub fn close_spl_account<'a, 'b, 'c, 'info>(
    owner: &AccountInfo<'info>, // The owner or authority authorized to close the account.
    destination: &AccountInfo<'info>, // The destination account where the remaining balance is transferred.
    close_account: &InterfaceAccount<'info, TokenAccount>, // The token account to be closed.
    token_program: &Program<'info, Token>, // The SPL token program.
    // token_program_2022: &Program<'info, Token2022>,
    signers_seeds: &[&[&[u8]]], // Signer seeds for program-derived addresses.
) -> Result<()> {
    let token_program_info = token_program.to_account_info();
    let close_account_info = close_account.to_account_info();
    // if close_account_info.owner == token_program_2022.key {
    //     token_program_info = token_program_2022.to_account_info()
    // }

    token_2022::close_account(CpiContext::new_with_signer(
        token_program_info,
        token_2022::CloseAccount {
            account: close_account_info,
            destination: destination.to_account_info(),
            authority: owner.to_account_info(),
        },
        signers_seeds,
    ))
}

pub fn burn<'a, 'b, 'c, 'info>(
    owner: &Signer<'info>, // The owner or authority authorized to burn tokens.
    mint: &InterfaceAccount<'info, Mint>, // The mint account for the tokens to be burned.
    burn_account: &InterfaceAccount<'info, TokenAccount>, // The token account to be burned.
    token_program: &Program<'info, Token>, // Standard token program for burning tokens
    // token_program_2022: &Program<'info, Token2022>, // Token-2022 program for burning tokens
    signers_seeds: &[&[&[u8]]], // Signer seeds for program-derived addresses.
    amount: u64,                // Amount of tokens to burn.
) -> Result<()> {
    let mint_info = mint.to_account_info();
    let token_program_info: AccountInfo<'_> = token_program.to_account_info();
    // if mint_info.owner == token_program_2022.key {
    //     token_program_info = token_program_2022.to_account_info()
    // }

    // Perform the burn operation using a cross-program invocation (CPI)
    // 1. Create a new CPI context with signer seeds (for PDA signing).
    // 2. Define the Burn context struct, specifying the mint, from account, and authority.
    token_2022::burn(
        CpiContext::new_with_signer(
            token_program_info, // Program to execute the burn, based on the conditions above
            token_2022::Burn {
                mint: mint_info,                      // Mint account from which the tokens originate
                from: burn_account.to_account_info(), // Account holding the tokens to burn
                authority: owner.to_account_info(),   // Authority account authorized to burn tokens
            },
            signers_seeds, // Signer seeds required for PDA authorization
        ),
        amount, // The number of tokens to burn
    )
}

/// Calculate the fee for output amount
/// This function calculates the inverse transfer fee based on the given post-fee amount and mint account.
// If the mint account is not controlled by the Token program, a default fee of 0 is returned.
pub fn get_transfer_inverse_fee(
    mint_account: Box<InterfaceAccount<Mint>>, // mint account that holds the mint data
    post_fee_amount: u64,                      // the amount after the fee has been applied
) -> Result<u64> {
    // Retrieve account info for the mint account
    let mint_info = mint_account.to_account_info();

    // Check if the mint account is owned by the Token program (sol_token_2022::Token::id)
    // If it is, there’s no custom fee applied, so we return 0 immediately
    if *mint_info.owner == Token::id() {
        return Ok(0);
    }

    // Attempt to borrow (read) the data from the mint account
    let mint_data = mint_info.try_borrow_data()?;

    // Unpack the mint data into a usable mint object with its extensions
    let mint = StateWithExtensions::<spl_token_2022::state::Mint>::unpack(&mint_data)?;

    // Initialize the fee variable to store the computed fee
    let fee = if let Ok(transfer_fee_config) = mint.get_extension::<TransferFeeConfig>() {
        let epoch = get_recent_epoch()?; // Get the current epoch for the fee calculation

        // Retrieve the fee configuration for the current epoch
        let transfer_fee = transfer_fee_config.get_epoch_fee(epoch);
        // Check if the fee is set to the maximum allowed basis points (in this case, a maximum fixed fee is used)
        if u16::from(transfer_fee.transfer_fee_basis_points) == MAX_FEE_BASIS_POINTS {
            u64::from(transfer_fee.maximum_fee)
        } else {
            // Calculate the inverse fee based on the given post-fee amount
            // Unwrap here assumes the calculation will succeed, which should be the case if the fee is set correctly
            transfer_fee_config
                .calculate_inverse_epoch_fee(epoch, post_fee_amount)
                .unwrap()
        }
    } else {
        0 // If there’s no transfer fee config extension, the fee is 0
    };
    // Return the computed fee
    Ok(fee)
}

/// Calculate the fee for input amount
/// This function calculates the transfer fee based on the given pre-fee amount and mint account.
pub fn get_transfer_fee(
    mint_account: Box<InterfaceAccount<Mint>>, // mint account holding mint data
    pre_fee_amount: u64,                       // the amount before the fee is applied
) -> Result<u64> {
    // returns a Result with the fee amount or an error if the fee calculation fails
    // Retrieve account info for the mint account
    let mint_info = mint_account.to_account_info();
    // Check if the mint account is owned by the standard Token program
    // If it is, then no custom transfer fee applies, so we return 0
    if *mint_info.owner == Token::id() {
        return Ok(0);
    }

    // Attempt to borrow (read) the data from the mint account
    let mint_data = mint_info.try_borrow_data()?;
    // Unpack the mint data to access the mint account's state with potential extensions
    let mint = StateWithExtensions::<spl_token_2022::state::Mint>::unpack(&mint_data)?;

    // Initialize the fee variable to calculate the transfer fee if a TransferFeeConfig is found
    let fee = if let Ok(transfer_fee_config) = mint.get_extension::<TransferFeeConfig>() {
        // Calculate the fee for the given amount based on the current epoch
        // Uses the configured epoch fee for this amount, and unwraps as it's expected to succeed
        transfer_fee_config
            .calculate_epoch_fee(get_recent_epoch()?, pre_fee_amount)
            .unwrap()
    } else {
        0
    };
    Ok(fee)
}

// This function checks if a mint account is supported based on ownership, whitelist, or allowed extensions.
pub fn is_supported_mint(mint_account: &InterfaceAccount<Mint>) -> Result<bool> {
    // Retrieve account information for the mint account
    let mint_info = mint_account.to_account_info();
    // Check if the mint account is owned by the Token program
    // If it is, we consider it supported, so return true
    if *mint_info.owner == Token::id() {
        return Ok(true);
    }

    // Define a whitelist of mint accounts that are explicitly supported
    let mint_whitelist: HashSet<&str> = MINT_WHITELIST.into_iter().collect();

    // Check if the mint account's key is in the whitelist
    // If it is, return true to indicate it’s supported
    if mint_whitelist.contains(mint_account.key().to_string().as_str()) {
        return Ok(true);
    }

    // Try to borrow (read) data from the mint account for further checks
    let mint_data = mint_info.try_borrow_data()?;

    // Unpack the mint data to access the mint's state with potential extensions
    let mint = StateWithExtensions::<spl_token_2022::state::Mint>::unpack(&mint_data)?;

    // Retrieve a list of all extension types associated with this mint account
    let extensions = mint.get_extension_types()?;

    // Check each extension type to ensure it’s within allowed types
    for e in extensions {
        if e != ExtensionType::TransferFeeConfig
            && e != ExtensionType::MetadataPointer
            && e != ExtensionType::TokenMetadata
        {
            // If any extension is not one of the allowed types, return false
            return Ok(false);
        }
    }
    // If all extensions are allowed types or if there are no disallowed extensions, return true
    Ok(true)
}
