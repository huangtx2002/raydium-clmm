use anchor_lang::{prelude::*, system_program};

pub fn create_or_allocate_account<'a>(
    program_id: &Pubkey, // The program ID for which this account is being created or allocated.
    payer: AccountInfo<'a>, // The account paying for the allocation or funding.
    system_program: AccountInfo<'a>, // System program to handle account creation.
    target_account: AccountInfo<'a>, // The target account that we’re creating or allocating.
    siger_seed: &[&[u8]], // Seeds used to generate the target account's PDA.
    space: usize,        // The amount of space to allocate to the target account.
) -> Result<()> {
    // Get rent information (to calculate minimum balance requirements).
    let rent = Rent::get()?;
    let current_lamports = target_account.lamports(); // Get current lamports (funds) in the target account.

    // Case 1: If the account is not funded (balance of zero), create it.
    if current_lamports == 0 {
        let lamports = rent.minimum_balance(space); // Minimum rent-exempt balance for `space` bytes.
                                                    // Prepare CPI accounts to create a new account.
        let cpi_accounts = system_program::CreateAccount {
            from: payer,
            to: target_account.clone(),
        };
        // Create a CPI context with signer information.
        let cpi_context = CpiContext::new(system_program.clone(), cpi_accounts);

        // Invoke the system program's `create_account` to fund and create the target account.
        system_program::create_account(
            cpi_context.with_signer(&[siger_seed]),
            lamports,
            u64::try_from(space).unwrap(),
            program_id,
        )?;
    } else {
        // Case 2: If the account is already funded, allocate space and assign it to the program ID.

        // Calculate additional lamports required, if the current balance is insufficient.
        let required_lamports = rent
            .minimum_balance(space)
            .max(1)
            .saturating_sub(current_lamports);
        if required_lamports > 0 {
            // Transfer any needed lamports to meet minimum balance requirements.
            let cpi_accounts = system_program::Transfer {
                from: payer.to_account_info(),
                to: target_account.clone(),
            };
            let cpi_context = CpiContext::new(system_program.clone(), cpi_accounts);
            system_program::transfer(cpi_context, required_lamports)?;
        }

        // Allocate the specified space for the target account.
        let cpi_accounts = system_program::Allocate {
            account_to_allocate: target_account.clone(),
        };
        let cpi_context = CpiContext::new(system_program.clone(), cpi_accounts);
        system_program::allocate(
            cpi_context.with_signer(&[siger_seed]),
            u64::try_from(space).unwrap(),
        )?;

        // Assign the target account to the given program ID.
        let cpi_accounts = system_program::Assign {
            account_to_assign: target_account.clone(),
        };
        let cpi_context = CpiContext::new(system_program.clone(), cpi_accounts);
        system_program::assign(cpi_context.with_signer(&[siger_seed]), program_id)?;
    }
    Ok(())
}

#[cfg(not(test))]
pub fn get_recent_epoch() -> Result<u64> {
    Ok(Clock::get()?.epoch)
}

#[cfg(test)]
pub fn get_recent_epoch() -> Result<u64> {
    use std::time::{SystemTime, UNIX_EPOCH};
    Ok(SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_secs()
        / (2 * 24 * 3600))
}
