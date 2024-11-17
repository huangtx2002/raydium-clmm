use anchor_lang::prelude::*;
use std::collections::HashSet;

pub const OPERATION_SEED: &str = "operation";
pub const OPERATION_SIZE_USIZE: usize = 10;
pub const WHITE_MINT_SIZE_USIZE: usize = 100;

/// Holds the current owner of the factory
#[account(zero_copy(unsafe))]
#[repr(packed)]
#[derive(Debug)]
pub struct OperationState {
    /// Bump to identify PDA
    pub bump: u8,
    /// Address of the operation owner
    pub operation_owners: [Pubkey; OPERATION_SIZE_USIZE],
    /// The mint address of whitelist to emmit reward
    pub whitelist_mints: [Pubkey; WHITE_MINT_SIZE_USIZE],
}

impl OperationState {
    pub const LEN: usize = 8 + 1 + 32 * OPERATION_SIZE_USIZE + 32 * WHITE_MINT_SIZE_USIZE;
    pub fn initialize(&mut self, bump: u8) {
        self.bump = bump;
        self.operation_owners = [Pubkey::default(); OPERATION_SIZE_USIZE];
        self.whitelist_mints = [Pubkey::default(); WHITE_MINT_SIZE_USIZE];
    }

    pub fn validate_operation_owner(&self, owner: Pubkey) -> bool {
        owner != Pubkey::default() && self.operation_owners.contains(&owner)
    }

    pub fn validate_whitelist_mint(&self, mint: Pubkey) -> bool {
        mint != Pubkey::default() && self.whitelist_mints.contains(&mint)
    }

    pub fn update_operation_owner(&mut self, keys: Vec<Pubkey>) {
        let mut operation_owners = self.operation_owners.to_vec();
        operation_owners.extend(keys.as_slice().iter());
        operation_owners.retain(|&item| item != Pubkey::default());
        let owners_set: HashSet<Pubkey> = HashSet::from_iter(operation_owners.iter().cloned());
        let mut updated_owner: Vec<Pubkey> = owners_set.into_iter().collect();
        updated_owner.sort_by(|a, b| a.cmp(b));

        //Verify that the length of updated_owner does not exceed OPERATION_SIZE_USIZE
        if updated_owner.len() > OPERATION_SIZE_USIZE {
            panic!("The total number of unique keys exceeds the allowed operation size.");
        }

        // clear
        self.operation_owners = [Pubkey::default(); OPERATION_SIZE_USIZE];
        // update
        self.operation_owners[0..updated_owner.len()].copy_from_slice(updated_owner.as_slice());
    }

    pub fn remove_operation_owner(&mut self, keys: Vec<Pubkey>) {
        let mut operation_owners: Vec<Pubkey> = self.operation_owners.to_vec();
        // remove keys from operation_owners
        operation_owners.retain(|x| !keys.contains(&x));
        // clear
        self.operation_owners = [Pubkey::default(); OPERATION_SIZE_USIZE];
        // update
        self.operation_owners[0..operation_owners.len()]
            .copy_from_slice(operation_owners.as_slice());
    }

    pub fn update_whitelist_mint(&mut self, keys: Vec<Pubkey>) {
        let mut whitelist_mints = self.whitelist_mints.to_vec();
        whitelist_mints.extend(keys.as_slice().iter());
        whitelist_mints.retain(|&item| item != Pubkey::default());
        let owners_set: HashSet<Pubkey> = HashSet::from_iter(whitelist_mints.iter().cloned());
        let updated_mints: Vec<Pubkey> = owners_set.into_iter().collect();

        if updated_mints.len() > WHITE_MINT_SIZE_USIZE {
            panic!("The total number of unique keys exceeds the allowed whitelist mint size.");
        }

        // clear
        self.whitelist_mints = [Pubkey::default(); WHITE_MINT_SIZE_USIZE];
        // update
        self.whitelist_mints[0..updated_mints.len()].copy_from_slice(updated_mints.as_slice());
    }

    pub fn remove_whitelist_mint(&mut self, keys: Vec<Pubkey>) {
        let mut whitelist_mints = self.whitelist_mints.to_vec();
        // remove keys from whitelist_mint
        whitelist_mints.retain(|x| !keys.contains(&x));
        // clear
        self.whitelist_mints = [Pubkey::default(); WHITE_MINT_SIZE_USIZE];
        // update
        self.whitelist_mints[0..whitelist_mints.len()].copy_from_slice(whitelist_mints.as_slice());
    }
}

#[cfg(test)]
mod test {
    use super::*;

    #[test]
    fn test_update_operation_owner_with_empty() {
        let mut operation_state = OperationState {
            bump: 0,
            operation_owners: [Pubkey::default(); OPERATION_SIZE_USIZE],
            whitelist_mints: [Pubkey::default(); WHITE_MINT_SIZE_USIZE],
        };
        let mut keys = Vec::new();
        keys.push(Pubkey::new_unique());
        keys.push(Pubkey::new_unique());
        keys.push(Pubkey::new_unique());
        keys.push(Pubkey::new_unique());
        keys.sort_by(|a, b| a.cmp(b));
        println!("{:?}", keys);

        operation_state.update_operation_owner(keys.clone());
        println!("{:?}", operation_state.operation_owners);
        assert_eq!(
            &keys.clone()[..],
            &operation_state.operation_owners[..keys.len()]
        );
    }

    #[test]
    fn test_update_operation_owner_with_not_empty() {
        let mut operation_state = OperationState {
            bump: 0,
            operation_owners: [Pubkey::default(); OPERATION_SIZE_USIZE],
            whitelist_mints: [Pubkey::default(); WHITE_MINT_SIZE_USIZE],
        };
        let existing_owner1 = Pubkey::new_unique();
        let existing_owner2 = Pubkey::new_unique();
        let existing_owner3 = Pubkey::new_unique();
        operation_state.operation_owners[0] = existing_owner1;
        operation_state.operation_owners[1] = existing_owner2;
        operation_state.operation_owners[2] = existing_owner3;
        let mut keys = Vec::new();
        keys.push(Pubkey::new_unique());
        keys.push(Pubkey::new_unique());
        keys.push(Pubkey::new_unique());
        keys.push(Pubkey::new_unique());
        keys.sort_by(|a, b| a.cmp(b));
        println!("{:?}", keys);

        operation_state.update_operation_owner(keys.clone());
        println!("{:?}", operation_state.operation_owners);

        // Combine the existing owners with the new keys
        let mut expected_owners = vec![existing_owner1, existing_owner2, existing_owner3];
        expected_owners.extend(keys.clone());
        let owners_set: HashSet<Pubkey> = HashSet::from_iter(expected_owners.iter().cloned());
        expected_owners = owners_set.into_iter().collect();
        expected_owners.sort_by(|a, b| a.cmp(b));

        // Verify that the updated operation_owners contains the correct keys
        let mut actual_owners = operation_state.operation_owners.to_vec();
        actual_owners.retain(|&item| item != Pubkey::default());
        actual_owners.sort_by(|a, b| a.cmp(b));
        assert_eq!(actual_owners, expected_owners);
    }

    #[test]
    fn test_update_operation_owner_with_repeat_key() {
        let mut operation_state = OperationState {
            bump: 0,
            operation_owners: [Pubkey::default(); OPERATION_SIZE_USIZE],
            whitelist_mints: [Pubkey::default(); WHITE_MINT_SIZE_USIZE],
        };
        operation_state.operation_owners[0] = Pubkey::new_unique();
        operation_state.operation_owners[1] = Pubkey::new_unique();
        operation_state.operation_owners[2] = Pubkey::new_unique();
        let mut keys = Vec::new();
        keys.push(operation_state.operation_owners[0]);
        keys.push(operation_state.operation_owners[1]);
        keys.push(Pubkey::new_unique());
        keys.push(Pubkey::new_unique());
        keys.sort_by(|a, b| a.cmp(b));
        println!("{:?}", keys);

        operation_state.update_operation_owner(keys.clone());
        println!("{:?}", operation_state.operation_owners);
    }

    #[test]
    fn test_update_operation_owner_with_full_array() {
        let mut operation_state = OperationState {
            bump: 0,
            operation_owners: [Pubkey::default(); OPERATION_SIZE_USIZE],
            whitelist_mints: [Pubkey::default(); WHITE_MINT_SIZE_USIZE],
        };
        let mut keys = Vec::new();
        for _i in 0..10 {
            keys.push(Pubkey::new_unique());
        }
        keys.sort_by(|a, b| a.cmp(b));
        println!("{:?}", keys);

        operation_state.update_operation_owner(keys.clone());
        println!("{:?}", operation_state.operation_owners);
        assert_eq!(
            &keys.clone()[..],
            &operation_state.operation_owners[..keys.len()]
        );
    }

    #[test]
    #[should_panic]
    fn test_update_operation_owner_with_over_flow_array() {
        let mut operation_state = OperationState {
            bump: 0,
            operation_owners: [Pubkey::default(); OPERATION_SIZE_USIZE],
            whitelist_mints: [Pubkey::default(); WHITE_MINT_SIZE_USIZE],
        };
        let mut keys = Vec::new();
        for _i in 0..11 {
            keys.push(Pubkey::new_unique());
        }
        keys.sort_by(|a, b| a.cmp(b));
        println!("{:?}", keys);

        operation_state.update_operation_owner(keys.clone());
    }

    #[test]
    fn test_remove_operator_owner() {
        let mut operation_state = OperationState {
            bump: 0,
            operation_owners: [Pubkey::default(); OPERATION_SIZE_USIZE],
            whitelist_mints: [Pubkey::default(); WHITE_MINT_SIZE_USIZE],
        };
        let mut keys = Vec::new();
        for _i in 0..3 {
            keys.push(Pubkey::new_unique());
        }
        keys.push(keys[0]);
        keys.sort_by(|a, b| a.cmp(b));
        operation_state.operation_owners[0..keys.len()].copy_from_slice(keys.clone().as_slice());
        operation_state.operation_owners[keys.len()] = Pubkey::new_unique();
        operation_state.operation_owners[keys.len() + 1] = Pubkey::new_unique();

        operation_state.remove_operation_owner(keys.clone());
        println!("{:?}", operation_state.operation_owners);
    }
}
