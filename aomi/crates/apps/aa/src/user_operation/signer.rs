use super::types::UserOperation;
use alloy_primitives::{Address, Bytes, FixedBytes, U256, keccak256};
use alloy_signer::{Signer, SignerSync};
use alloy_signer_local::PrivateKeySigner;
use alloy_sol_types::SolValue;
use eyre::Result;

pub struct UserOperationSigner {
    signer: PrivateKeySigner,
    entry_point: Address,
    chain_id: U256,
}

impl UserOperationSigner {
    pub fn new(private_key: FixedBytes<32>, entry_point: Address, chain_id: u64) -> Result<Self> {
        let signer = PrivateKeySigner::from_bytes(&private_key)?;
        Ok(Self {
            signer,
            entry_point,
            chain_id: U256::from(chain_id),
        })
    }

    /// Get the signer's address
    pub fn address(&self) -> Address {
        self.signer.address()
    }

    /// Compute UserOperation hash (ERC-4337 v0.7)
    /// Following UserOperationLib.sol from eth-infinitism/account-abstraction:
    /// 1. Hash dynamic fields (initCode, callData, paymasterAndData)
    /// 2. Encode tuple: (sender, nonce, hashInitCode, hashCallData, accountGasLimits, preVerificationGas, gasFees, hashPaymasterAndData)
    /// 3. Hash that encoding
    /// 4. Encode: (hash, entryPoint, chainId)
    /// 5. Hash final encoding
    pub fn hash_user_op(&self, user_op: &UserOperation) -> FixedBytes<32> {
        let packed = user_op.pack();

        // Step 1: Hash the dynamic fields (matching UserOperationLib.encode)
        let hash_init_code = keccak256(&packed.initCode);
        let hash_call_data = keccak256(&packed.callData);
        let hash_paymaster_and_data = keccak256(&packed.paymasterAndData);

        // Step 2: ABI-encode the tuple with hashed fields
        // This matches: abi.encode(sender, nonce, hashInitCode, hashCallData, accountGasLimits, preVerificationGas, gasFees, hashPaymasterAndData)
        let encoded_tuple = (
            packed.sender,
            packed.nonce,
            hash_init_code,
            hash_call_data,
            packed.accountGasLimits,
            packed.preVerificationGas,
            packed.gasFees,
            hash_paymaster_and_data,
        )
            .abi_encode();

        // Step 3: Hash the encoded tuple (this is userOp.hash())
        let packed_hash = keccak256(&encoded_tuple);

        // Step 4: ABI-encode the hash with EntryPoint and chainId
        let final_encoded = (packed_hash, self.entry_point, self.chain_id).abi_encode();

        // Step 5: Hash to get getUserOpHash
        keccak256(&final_encoded)
    }

    /// Sign UserOperation and populate the signature field
    /// Uses EIP-191 eth_sign format (adds "\x19Ethereum Signed Message:\n32" prefix)
    pub async fn sign(&self, user_op: &mut UserOperation) -> Result<()> {
        let hash = self.hash_user_op(user_op);

        // Sign with eth_sign format (EIP-191)
        let signature = self.signer.sign_hash(&hash).await?;

        // Convert signature to bytes
        user_op.signature = Bytes::from(signature.as_bytes());

        Ok(())
    }

    /// Sign UserOperation synchronously
    pub fn sign_sync(&self, user_op: &mut UserOperation) -> Result<()> {
        let hash = self.hash_user_op(user_op);

        // Sign with eth_sign format (EIP-191)
        // Use sign_message which adds the "\x19Ethereum Signed Message:\n32" prefix
        let signature = self.signer.sign_message_sync(hash.as_slice())?;

        // Convert signature to bytes
        user_op.signature = Bytes::from(signature.as_bytes());

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use alloy_primitives::U256;

    #[test]
    fn test_signer_creation() {
        let private_key = FixedBytes::from([0x42; 32]);
        let entry_point = Address::from([0x01; 20]);
        let chain_id = 31337;

        let signer = UserOperationSigner::new(private_key, entry_point, chain_id);
        assert!(signer.is_ok());
    }

    #[test]
    fn test_hash_user_op() {
        let private_key = FixedBytes::from([0x42; 32]);
        let entry_point = Address::from([0x01; 20]);
        let chain_id = 31337;

        let signer = UserOperationSigner::new(private_key, entry_point, chain_id).unwrap();

        let user_op = UserOperation {
            sender: Address::ZERO,
            nonce: U256::from(1),
            factory: None,
            factory_data: None,
            call_data: Bytes::from(vec![0x12, 0x34]),
            call_gas_limit: U256::from(100000),
            verification_gas_limit: U256::from(200000),
            pre_verification_gas: U256::from(50000),
            max_fee_per_gas: U256::from(1000000000),
            max_priority_fee_per_gas: U256::from(1000000000),
            paymaster: None,
            paymaster_verification_gas_limit: None,
            paymaster_post_op_gas_limit: None,
            paymaster_data: None,
            signature: Bytes::new(),
        };

        let hash = signer.hash_user_op(&user_op);

        // Hash should be 32 bytes
        assert_eq!(hash.len(), 32);

        // Hash should be deterministic
        let hash2 = signer.hash_user_op(&user_op);
        assert_eq!(hash, hash2);
    }

    #[test]
    fn test_sign_sync() {
        let private_key = FixedBytes::from([0x42; 32]);
        let entry_point = Address::from([0x01; 20]);
        let chain_id = 31337;

        let signer = UserOperationSigner::new(private_key, entry_point, chain_id).unwrap();

        let mut user_op = UserOperation {
            sender: Address::ZERO,
            nonce: U256::from(1),
            factory: None,
            factory_data: None,
            call_data: Bytes::from(vec![0x12, 0x34]),
            call_gas_limit: U256::from(100000),
            verification_gas_limit: U256::from(200000),
            pre_verification_gas: U256::from(50000),
            max_fee_per_gas: U256::from(1000000000),
            max_priority_fee_per_gas: U256::from(1000000000),
            paymaster: None,
            paymaster_verification_gas_limit: None,
            paymaster_post_op_gas_limit: None,
            paymaster_data: None,
            signature: Bytes::new(),
        };

        let result = signer.sign_sync(&mut user_op);
        assert!(result.is_ok());

        // Signature should be 65 bytes (r, s, v)
        assert_eq!(user_op.signature.len(), 65);
    }
}
