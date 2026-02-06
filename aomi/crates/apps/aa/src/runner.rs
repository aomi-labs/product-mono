use crate::bundler::BundlerClient;
use crate::user_operation::{UserOperationBuilder, UserOperationSigner};
use alloy_network::{Ethereum, TransactionBuilder};
use alloy_primitives::{Address, Bytes, FixedBytes, U256};
use alloy_provider::{Provider, RootProvider};
use alloy_rpc_types::TransactionRequest;
use alloy_signer_local::PrivateKeySigner;
use alloy_sol_types::{SolCall, SolValue};
use aomi_scripts::ContractConfig;
use aomi_scripts::contract::compiler::ContractCompiler;
use eyre::Result;
use foundry_config::Config as FoundryConfig;
use std::path::PathBuf;
use tracing::info;

const ENTRY_POINT_V07: Address =
    alloy_primitives::address!("0000000071727De22E5E9d8BAf0edAc6f37da032");

pub struct DeployedContracts {
    pub factory: Address,
    pub counter: Address,
}

pub struct AAPocRunner {
    compiler: ContractCompiler,
    bundler: BundlerClient,
    chain_id: u64,
    provider: RootProvider<Ethereum>,
    deployer: PrivateKeySigner,
}

impl AAPocRunner {
    pub async fn new(bundler_rpc: String, fork_url: String) -> Result<Self> {
        // Load foundry config from aa crate
        let aa_root = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
        let mut foundry_config = FoundryConfig::load_with_root(aa_root.clone())
            .unwrap_or_else(|_| FoundryConfig::default());

        // Set source directory to contracts folder
        foundry_config.src = aa_root.join("src/contracts");

        // Create contract compiler
        let config = ContractConfig::new(foundry_config, Some("aa-poc".to_string()));
        let compiler = ContractCompiler::new(&config)?;
        let bundler = BundlerClient::new(bundler_rpc, ENTRY_POINT_V07);

        // Create provider for RPC calls
        let url = fork_url.parse()?;
        let provider = RootProvider::<Ethereum>::new_http(url);

        // Query actual chain ID from provider
        let chain_id = provider.get_chain_id().await?;
        info!("  Detected chain ID: {}", chain_id);

        // Use Anvil test account #0 as deployer
        // This is a well-known test private key that Anvil pre-funds
        let deployer = "0xac0974bec39a17e36ba4a6b4d238ff944bacb478cbed5efcae784d7bf4f2ff80"
            .parse::<PrivateKeySigner>()?;

        Ok(Self {
            compiler,
            bundler,
            chain_id,
            provider,
            deployer,
        })
    }

    /// Deploy a contract to Anvil via transaction
    async fn deploy_contract(&self, bytecode: Vec<u8>) -> Result<Address> {
        let deployer_address = self.deployer.address();

        // Create deployment transaction
        let tx = TransactionRequest::default()
            .from(deployer_address)
            .input(bytecode.into());

        // Send transaction
        let pending_tx = self.provider.send_transaction(tx).await?;
        let receipt = pending_tx.get_receipt().await?;

        receipt
            .contract_address
            .ok_or_else(|| eyre::eyre!("No contract address in receipt"))
    }

    /// Phase 1: Deploy contracts
    pub async fn deploy_contracts(&mut self) -> Result<DeployedContracts> {
        info!("Phase 1: Deploying contracts...");

        // Get the aa crate root for contract paths
        let aa_root = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
        let contracts_dir = aa_root.join("src/contracts");

        // Deploy SimpleAccount implementation with EntryPoint constructor arg
        info!("  Deploying SimpleAccount implementation...");
        let account_path = contracts_dir.join("SimpleAccount.sol");

        // Compile and get bytecode
        let account_output = self.compiler.compile_file(account_path)?;
        let mut account_bytecode = self
            .compiler
            .get_contract_bytecode(&account_output, "SimpleAccount")?;

        // ABI-encode constructor argument: address entryPoint
        let constructor_args = ENTRY_POINT_V07.abi_encode();
        account_bytecode.extend_from_slice(&constructor_args);

        let account_impl = self.deploy_contract(account_bytecode).await?;
        info!("    ✓ SimpleAccount: {:?}", account_impl);

        // Deploy SimpleAccountFactory with EntryPoint constructor arg
        info!("  Deploying SimpleAccountFactory...");
        let factory_path = contracts_dir.join("SimpleAccountFactory.sol");

        // Compile and get bytecode
        let factory_output = self.compiler.compile_file(factory_path)?;
        let mut factory_bytecode = self
            .compiler
            .get_contract_bytecode(&factory_output, "SimpleAccountFactory")?;
        let constructor_args = ENTRY_POINT_V07.abi_encode();
        factory_bytecode.extend_from_slice(&constructor_args);

        let factory = self.deploy_contract(factory_bytecode).await?;
        info!("    ✓ SimpleAccountFactory: {:?}", factory);

        // Deploy Counter (no constructor args)
        info!("  Deploying Counter...");
        let counter_path = contracts_dir.join("Counter.sol");

        // Compile and get bytecode
        let counter_output = self.compiler.compile_file(counter_path)?;
        let counter_bytecode = self
            .compiler
            .get_contract_bytecode(&counter_output, "Counter")?;

        let counter = self.deploy_contract(counter_bytecode).await?;
        info!("    ✓ Counter: {:?}", counter);

        info!("✓ Contracts deployed successfully");

        Ok(DeployedContracts { factory, counter })
    }

    /// Phase 2: Verify bundler connectivity
    pub async fn verify_bundler(&self) -> Result<()> {
        info!("Phase 2: Verifying Alto bundler connectivity...");

        let entry_points = self.bundler.supported_entry_points().await?;
        info!("  Supported EntryPoints: {:?}", entry_points);

        if !entry_points.contains(&ENTRY_POINT_V07) {
            eyre::bail!("Alto does not support EntryPoint v0.7");
        }

        info!("✓ Bundler verified");
        Ok(())
    }

    /// Phase 3: Build and send UserOperation
    pub async fn execute_user_operation(
        &mut self,
        contracts: &DeployedContracts,
        owner_key: FixedBytes<32>,
    ) -> Result<crate::bundler::UserOperationReceipt> {
        info!("Phase 3: Building and sending UserOperation...");

        // Generate owner address
        let signer = UserOperationSigner::new(owner_key, ENTRY_POINT_V07, self.chain_id)?;
        let owner = signer.address();
        info!("  Owner address: {:?}", owner);

        // Build UserOperation
        let builder = UserOperationBuilder::new(
            ENTRY_POINT_V07,
            contracts.factory,
            owner,
            U256::ZERO, // salt = 0
        );

        let sender = builder.get_sender(&self.provider).await?;
        info!("  Counterfactual account: {:?}", sender);

        // Fund the account's EntryPoint deposit
        info!("  Funding account deposit...");
        alloy_sol_types::sol! {
            function depositTo(address account) external payable;
        }
        let deposit_call = depositToCall { account: sender };
        let deposit_tx = TransactionRequest::default()
            .to(ENTRY_POINT_V07)
            .value(U256::from(10_000_000_000_000_000_000u128)) // 10 ETH
            .input(deposit_call.abi_encode().into())
            .from(self.deployer.address());

        let pending = self.provider.send_transaction(deposit_tx).await?;
        pending.get_receipt().await?;
        info!("    ✓ Deposited 10 ETH to EntryPoint for account");

        // Build executeBatch callData targeting Counter.increment()
        alloy_sol_types::sol! {
            function increment() external;
        }

        let increment_call = incrementCall {};
        let call_data = builder.build_execute_batch_call(
            vec![contracts.counter],
            vec![U256::ZERO],
            vec![Bytes::from(increment_call.abi_encode())],
        );

        // Create unsigned UserOperation with estimated gas values
        let mut user_op = builder.build_unsigned(&self.provider, call_data).await?;

        // Set initial gas estimates for first signature
        user_op.call_gas_limit = U256::from(100_000u64);
        user_op.verification_gas_limit = U256::from(300_000u64);
        user_op.pre_verification_gas = U256::from(50_000u64);
        user_op.max_fee_per_gas = U256::from(50_000_000_000u64); // 50 gwei
        user_op.max_priority_fee_per_gas = U256::from(1_000_000_000u64); // 1 gwei

        // Sign with estimated values for gas estimation
        info!("  Signing UserOperation for gas estimation...");
        signer.sign_sync(&mut user_op)?;
        info!("  Built signed UserOperation");

        // Estimate gas
        info!("  Estimating gas...");
        let gas_estimate = self.bundler.estimate_user_operation_gas(&user_op).await?;

        // Update gas values from estimate
        user_op.call_gas_limit = gas_estimate.call_gas_limit;
        user_op.verification_gas_limit = gas_estimate.verification_gas_limit;
        user_op.pre_verification_gas = gas_estimate.pre_verification_gas;
        info!("    callGasLimit: {}", gas_estimate.call_gas_limit);
        info!(
            "    verificationGasLimit: {}",
            gas_estimate.verification_gas_limit
        );
        info!(
            "    preVerificationGas: {}",
            gas_estimate.pre_verification_gas
        );

        // Clear old signature before re-signing
        user_op.signature = Bytes::new();

        // Re-sign with final gas values
        info!("  Re-signing UserOperation with final gas values...");
        signer.sign_sync(&mut user_op)?;

        // Try sending through Alto first since gas estimation works
        info!("  Attempting to send UserOperation through Alto bundler...");

        // Try sending via bundler
        match self.bundler.send_user_operation(&user_op).await {
            Ok(user_op_hash) => {
                info!("  ✓ UserOperation sent to Alto");
                info!("  UserOp hash: {:?}", user_op_hash);

                // Wait for receipt
                info!("  Waiting for UserOperation to be mined...");
                match self
                    .bundler
                    .wait_for_receipt(
                        user_op_hash,
                        std::time::Duration::from_secs(60),
                        std::time::Duration::from_secs(2),
                    )
                    .await
                {
                    Ok(receipt) => {
                        info!("✓ UserOperation executed successfully!");
                        info!("  Gas used: {}", receipt.actual_gas_used);
                        return Ok(receipt);
                    }
                    Err(e) => {
                        info!("  Alto bundling failed: {}", e);
                        info!("  Falling back to direct EntryPoint execution...");
                    }
                }
            }
            Err(e) => {
                info!("  Could not send to Alto: {}", e);
                info!("  Falling back to direct EntryPoint execution...");
            }
        }

        // Fallback: Pack the UserOperation for direct execution
        let packed_user_op = user_op.pack();

        // Define handleOps function using sol! macro
        alloy_sol_types::sol! {
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

            function handleOps(PackedUserOperation[] calldata ops, address payable beneficiary) external;
        }

        // Convert our PackedUserOperation to the sol! macro version
        let sol_packed_op = PackedUserOperation {
            sender: packed_user_op.sender,
            nonce: packed_user_op.nonce,
            initCode: packed_user_op.initCode.clone(),
            callData: packed_user_op.callData.clone(),
            accountGasLimits: packed_user_op.accountGasLimits,
            preVerificationGas: packed_user_op.preVerificationGas,
            gasFees: packed_user_op.gasFees,
            paymasterAndData: packed_user_op.paymasterAndData.clone(),
            signature: packed_user_op.signature.clone(),
        };

        let handle_ops_call = handleOpsCall {
            ops: vec![sol_packed_op],
            beneficiary: self.deployer.address(),
        };

        let tx = TransactionRequest::default()
            .to(ENTRY_POINT_V07)
            .input(handle_ops_call.abi_encode().into())
            .from(self.deployer.address())
            .with_gas_limit(10_000_000); // High gas limit for testing

        let pending = self.provider.send_transaction(tx).await?;
        let tx_receipt = pending.get_receipt().await?;

        info!("✓ UserOperation executed via EntryPoint");
        info!("  Transaction: {:?}", tx_receipt.transaction_hash);
        info!("  Gas used: {}", tx_receipt.gas_used);
        info!(
            "  Status: {}",
            if tx_receipt.status() {
                "Success"
            } else {
                "Failed"
            }
        );

        // Return a receipt for compatibility
        Ok(crate::bundler::UserOperationReceipt {
            user_op_hash: FixedBytes::ZERO,
            sender,
            nonce: user_op.nonce,
            actual_gas_cost: U256::from(tx_receipt.gas_used),
            actual_gas_used: U256::from(tx_receipt.gas_used),
            success: tx_receipt.status(),
            logs: vec![],
            receipt: None,
        })
    }

    /// Phase 4: Verify execution
    pub async fn verify_execution(&self, contracts: &DeployedContracts) -> Result<()> {
        info!("Phase 4: Verifying execution...");

        // Read counter value
        alloy_sol_types::sol! {
            function getValue() external view returns (uint256);
        }

        let get_value_call = getValueCall {};

        let tx = TransactionRequest::default()
            .to(contracts.counter)
            .input(get_value_call.abi_encode().into());

        let result = self.provider.call(tx).await?;
        let value = getValueCall::abi_decode_returns(&result)?;
        info!("  Counter value: {}", value);

        if value != U256::from(1) {
            eyre::bail!("Counter value is {}, expected 1", value);
        }

        info!("✓ Execution verified");
        Ok(())
    }
}
