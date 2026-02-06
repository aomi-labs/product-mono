use alloy_primitives::{Address, Bytes, FixedBytes, U256};
use alloy_sol_types::sol;
use serde::{Deserialize, Serialize};

// Define ERC-4337 v0.7 PackedUserOperation using Alloy sol! macro
sol! {
    #[derive(Debug, Serialize, Deserialize)]
    struct PackedUserOperation {
        address sender;
        uint256 nonce;
        bytes initCode;
        bytes callData;
        bytes32 accountGasLimits;
        uint256 preVerificationGas;
        bytes32 gasFees;
        bytes paymasterAndData;
        bytes signature;
    }
}

/// Unpacked UserOperation for easier manipulation before packing
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct UserOperation {
    pub sender: Address,
    pub nonce: U256,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub factory: Option<Address>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub factory_data: Option<Bytes>,
    pub call_data: Bytes,
    pub call_gas_limit: U256,
    pub verification_gas_limit: U256,
    pub pre_verification_gas: U256,
    pub max_fee_per_gas: U256,
    pub max_priority_fee_per_gas: U256,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub paymaster: Option<Address>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub paymaster_verification_gas_limit: Option<U256>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub paymaster_post_op_gas_limit: Option<U256>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub paymaster_data: Option<Bytes>,
    pub signature: Bytes,
}

impl UserOperation {
    /// Pack into v0.7 format for EntryPoint
    pub fn pack(&self) -> PackedUserOperation {
        // Construct initCode: factory address + factoryData (or empty if no factory)
        let init_code = if let (Some(factory), Some(data)) = (&self.factory, &self.factory_data) {
            let mut init = Vec::with_capacity(20 + data.len());
            init.extend_from_slice(factory.as_slice());
            init.extend_from_slice(data.as_ref());
            Bytes::from(init)
        } else {
            Bytes::new()
        };

        // Pack gas limits: [16 bytes verificationGasLimit][16 bytes callGasLimit]
        let account_gas_limits = pack_u128_pair(
            u256_to_u128(self.verification_gas_limit),
            u256_to_u128(self.call_gas_limit),
        );

        // Pack fees: [16 bytes maxPriorityFee][16 bytes maxFee]
        let gas_fees = pack_u128_pair(
            u256_to_u128(self.max_priority_fee_per_gas),
            u256_to_u128(self.max_fee_per_gas),
        );

        // Construct paymasterAndData
        let paymaster_and_data = if let Some(paymaster) = &self.paymaster {
            let verification_gas = self.paymaster_verification_gas_limit.unwrap_or(U256::ZERO);
            let post_op_gas = self.paymaster_post_op_gas_limit.unwrap_or(U256::ZERO);
            let data = self
                .paymaster_data
                .as_ref()
                .map(|d| d.as_ref())
                .unwrap_or(&[]);

            let mut packed = Vec::with_capacity(20 + 16 + 16 + data.len());
            packed.extend_from_slice(paymaster.as_slice()); // 20 bytes

            // Pack verification and post-op gas limits as u128
            packed.extend_from_slice(&u256_to_u128(verification_gas).to_be_bytes());
            packed.extend_from_slice(&u256_to_u128(post_op_gas).to_be_bytes());
            packed.extend_from_slice(data);

            Bytes::from(packed)
        } else {
            Bytes::new()
        };

        PackedUserOperation {
            sender: self.sender,
            nonce: self.nonce,
            initCode: init_code,
            callData: self.call_data.clone(),
            accountGasLimits: account_gas_limits,
            preVerificationGas: self.pre_verification_gas,
            gasFees: gas_fees,
            paymasterAndData: paymaster_and_data,
            signature: self.signature.clone(),
        }
    }
}

/// Pack two u128 values into a bytes32
fn pack_u128_pair(high: u128, low: u128) -> FixedBytes<32> {
    let mut bytes = [0u8; 32];
    bytes[0..16].copy_from_slice(&high.to_be_bytes());
    bytes[16..32].copy_from_slice(&low.to_be_bytes());
    FixedBytes::from(bytes)
}

/// Convert U256 to u128, panicking if it doesn't fit
fn u256_to_u128(value: U256) -> u128 {
    value.try_into().expect("Value too large to fit in u128")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_pack_u128_pair() {
        let high: u128 = 0x1234567890ABCDEF_FEDCBA0987654321;
        let low: u128 = 0xAAAABBBBCCCCDDDD_EEEEFFFFAAAABBBB;

        let packed = pack_u128_pair(high, low);

        // Verify high bytes
        assert_eq!(&packed[0..16], &high.to_be_bytes());
        // Verify low bytes
        assert_eq!(&packed[16..32], &low.to_be_bytes());
    }

    #[test]
    fn test_user_operation_pack_no_factory() {
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
            signature: Bytes::from(vec![0xFF; 65]),
        };

        let packed = user_op.pack();

        // initCode should be empty
        assert_eq!(packed.initCode.len(), 0);

        // paymasterAndData should be empty
        assert_eq!(packed.paymasterAndData.len(), 0);

        // callData should match
        assert_eq!(packed.callData, user_op.call_data);
    }

    #[test]
    fn test_user_operation_pack_with_factory() {
        let factory = Address::from([0x11; 20]);
        let factory_data = Bytes::from(vec![0xAA, 0xBB, 0xCC]);

        let user_op = UserOperation {
            sender: Address::ZERO,
            nonce: U256::from(1),
            factory: Some(factory),
            factory_data: Some(factory_data.clone()),
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
            signature: Bytes::from(vec![0xFF; 65]),
        };

        let packed = user_op.pack();

        // initCode should be factory address + factory_data
        assert_eq!(packed.initCode.len(), 20 + 3);
        assert_eq!(&packed.initCode[0..20], factory.as_slice());
        assert_eq!(&packed.initCode[20..23], factory_data.as_ref());
    }
}
