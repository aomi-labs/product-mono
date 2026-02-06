pub mod abi_encoder;
pub mod account;
pub mod cast;
pub mod etherscan;
pub mod gateway;
pub mod wallet;

// Gateway implementations (conditionally compiled)
#[cfg(not(any(test, feature = "eval-test")))]
pub mod production;

#[cfg(any(test, feature = "eval-test"))]
pub mod local;

// Re-export gateway types for convenience
pub use gateway::{
    AccountInfo, Erc20BalanceResult, EvmGateway, WalletTransactionResult, get_gateway,
};
