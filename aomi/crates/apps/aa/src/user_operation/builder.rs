use super::types::UserOperation;
use alloy_primitives::{Address, Bytes, U256};
use alloy_provider::Provider;
use alloy_rpc_types::TransactionRequest;
use alloy_sol_types::SolCall;
use eyre::Result;

// Define SimpleAccount interface for building callData
alloy_sol_types::sol! {
    interface ISimpleAccount {
        function execute(address dest, uint256 value, bytes calldata func) external;
        function executeBatch(address[] calldata dest, uint256[] calldata value, bytes[] calldata func) external;
        function getNonce() external view returns (uint256);
    }

    interface ISimpleAccountFactory {
        function createAccount(address owner, uint256 salt) external returns (address);
        function getAddress(address owner, uint256 salt) external view returns (address);
    }

    interface IEntryPoint {
        function getNonce(address sender, uint192 key) external view returns (uint256);
    }
}

pub struct UserOperationBuilder {
    entry_point: Address,
    factory: Address,
    owner: Address,
    salt: U256,
}

impl UserOperationBuilder {
    pub fn new(entry_point: Address, factory: Address, owner: Address, salt: U256) -> Self {
        Self {
            entry_point,
            factory,
            owner,
            salt,
        }
    }

    /// Compute counterfactual account address by calling factory.getAddress
    pub async fn get_sender<P: Provider + Clone>(&self, provider: &P) -> Result<Address> {
        let call = ISimpleAccountFactory::getAddressCall {
            owner: self.owner,
            salt: self.salt,
        };

        let tx = TransactionRequest::default()
            .to(self.factory)
            .input(call.abi_encode().into());

        let result = provider.call(tx).await?;

        // Debug: log the result
        tracing::debug!("getAddress result: {:?}, len: {}", result, result.len());

        // Decode address from result - use SolCall::abi_decode_returns
        let address = ISimpleAccountFactory::getAddressCall::abi_decode_returns(&result)?;
        Ok(address)
    }

    /// Build factory initCode (factory address + factoryData)
    pub fn build_init_code(&self) -> (Address, Bytes) {
        let factory_data = ISimpleAccountFactory::createAccountCall {
            owner: self.owner,
            salt: self.salt,
        }
        .abi_encode();

        tracing::debug!("InitCode factory: {:?}", self.factory);
        tracing::debug!(
            "InitCode factoryData: {:?}",
            Bytes::from(factory_data.clone())
        );

        (self.factory, Bytes::from(factory_data))
    }

    /// Build callData for executeBatch
    pub fn build_execute_batch_call(
        &self,
        targets: Vec<Address>,
        values: Vec<U256>,
        data: Vec<Bytes>,
    ) -> Bytes {
        ISimpleAccount::executeBatchCall {
            dest: targets,
            value: values,
            func: data,
        }
        .abi_encode()
        .into()
    }

    /// Build callData for single execute
    pub fn build_execute_call(&self, target: Address, value: U256, data: Bytes) -> Bytes {
        ISimpleAccount::executeCall {
            dest: target,
            value,
            func: data,
        }
        .abi_encode()
        .into()
    }

    /// Get account nonce from EntryPoint
    pub async fn get_nonce<P: Provider + Clone>(
        &self,
        provider: &P,
        sender: Address,
    ) -> Result<U256> {
        let call = IEntryPoint::getNonceCall {
            sender,
            key: alloy_primitives::Uint::from(0u64), // Using key 0 for simplicity
        };

        let tx = TransactionRequest::default()
            .to(self.entry_point)
            .input(call.abi_encode().into());

        let result = provider.call(tx).await?;

        // Decode nonce from result - use SolCall::abi_decode_returns
        let nonce = IEntryPoint::getNonceCall::abi_decode_returns(&result)?;
        Ok(nonce)
    }

    /// Create unsigned UserOperation
    pub async fn build_unsigned<P: Provider + Clone>(
        &self,
        provider: &P,
        call_data: Bytes,
    ) -> Result<UserOperation> {
        let sender = self.get_sender(provider).await?;
        let nonce = self.get_nonce(provider, sender).await.unwrap_or(U256::ZERO);
        let (factory, factory_data) = self.build_init_code();

        Ok(UserOperation {
            sender,
            nonce,
            factory: Some(factory),
            factory_data: Some(factory_data),
            call_data,
            // Gas values will be filled by bundler estimation
            call_gas_limit: U256::ZERO,
            verification_gas_limit: U256::ZERO,
            pre_verification_gas: U256::ZERO,
            max_fee_per_gas: U256::ZERO,
            max_priority_fee_per_gas: U256::ZERO,
            // No paymaster for POC
            paymaster: None,
            paymaster_verification_gas_limit: None,
            paymaster_post_op_gas_limit: None,
            paymaster_data: None,
            signature: Bytes::new(), // Will be filled after signing
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_build_init_code() {
        let entry_point = Address::from([0x01; 20]);
        let factory = Address::from([0x02; 20]);
        let owner = Address::from([0x03; 20]);
        let salt = U256::from(42);

        let builder = UserOperationBuilder::new(entry_point, factory, owner, salt);
        let (factory_addr, factory_data) = builder.build_init_code();

        assert_eq!(factory_addr, factory);
        assert!(!factory_data.is_empty()); // Should contain ABI-encoded createAccount call
    }

    #[test]
    fn test_build_execute_call() {
        let entry_point = Address::from([0x01; 20]);
        let factory = Address::from([0x02; 20]);
        let owner = Address::from([0x03; 20]);
        let salt = U256::from(42);

        let builder = UserOperationBuilder::new(entry_point, factory, owner, salt);

        let target = Address::from([0x04; 20]);
        let value = U256::ZERO;
        let data = Bytes::from(vec![0xAA, 0xBB]);

        let call_data = builder.build_execute_call(target, value, data);
        assert!(!call_data.is_empty());
    }

    #[test]
    fn test_build_execute_batch_call() {
        let entry_point = Address::from([0x01; 20]);
        let factory = Address::from([0x02; 20]);
        let owner = Address::from([0x03; 20]);
        let salt = U256::from(42);

        let builder = UserOperationBuilder::new(entry_point, factory, owner, salt);

        let targets = vec![Address::from([0x04; 20]), Address::from([0x05; 20])];
        let values = vec![U256::ZERO, U256::from(1000)];
        let data = vec![Bytes::from(vec![0xAA]), Bytes::from(vec![0xBB])];

        let call_data = builder.build_execute_batch_call(targets, values, data);
        assert!(!call_data.is_empty());
    }
}
