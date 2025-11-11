use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Contract {
    pub address: String,
    pub chain: String,
    pub chain_id: i32,
    pub name: String,
    pub symbol: Option<String>,
    pub description: Option<String>,
    pub is_proxy: bool,
    pub implementation_address: Option<String>,
    pub source_code: String,
    pub abi: String,
    pub tvl: Option<f64>,
    pub transaction_count: Option<i64>,
    pub last_activity_at: Option<i64>,
    pub data_source: DataSource,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum DataSource {
    DefiLlama,
    CoinGecko,
    Etherscan,
    Manual,
}

impl DataSource {
    pub fn as_str(&self) -> &str {
        match self {
            DataSource::DefiLlama => "defillama",
            DataSource::CoinGecko => "coingecko",
            DataSource::Etherscan => "etherscan",
            DataSource::Manual => "manual",
        }
    }

    pub fn from_str(s: &str) -> Option<Self> {
        match s.to_lowercase().as_str() {
            "defillama" => Some(DataSource::DefiLlama),
            "coingecko" => Some(DataSource::CoinGecko),
            "etherscan" => Some(DataSource::Etherscan),
            "manual" => Some(DataSource::Manual),
            _ => None,
        }
    }
}

impl std::fmt::Display for DataSource {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.as_str())
    }
}

#[derive(Debug, Clone)]
pub struct EnrichedContract {
    pub metadata: ContractMetadata,
    pub code: ContractCode,
    pub metrics: ContractMetrics,
}

#[derive(Debug, Clone)]
pub struct ContractMetadata {
    pub address: String,
    pub chain_id: i32,
    pub chain: String,
    pub name: String,
    pub symbol: Option<String>,
    pub description: Option<String>,
}

#[derive(Debug, Clone)]
pub struct ContractCode {
    pub source_code: String,
    pub abi: String,
    pub is_proxy: bool,
    pub implementation_address: Option<String>,
}

#[derive(Debug, Clone)]
pub struct ContractMetrics {
    pub tvl: Option<f64>,
    pub transaction_count: Option<i64>,
    pub last_activity_at: Option<i64>,
}

impl EnrichedContract {
    /// Convert an EnrichedContract into a Contract
    pub fn into_contract(self, data_source: DataSource) -> Contract {
        Contract {
            address: self.metadata.address,
            chain: self.metadata.chain,
            chain_id: self.metadata.chain_id,
            name: self.metadata.name,
            symbol: self.metadata.symbol,
            description: self.metadata.description,
            is_proxy: self.code.is_proxy,
            implementation_address: self.code.implementation_address,
            source_code: self.code.source_code,
            abi: self.code.abi,
            tvl: self.metrics.tvl,
            transaction_count: self.metrics.transaction_count,
            last_activity_at: self.metrics.last_activity_at,
            data_source,
        }
    }
}

impl Contract {
    /// Create a new Contract with required fields
    pub fn new(
        address: String,
        chain: String,
        chain_id: i32,
        name: String,
        source_code: String,
        abi: String,
        data_source: DataSource,
    ) -> Self {
        Self {
            address,
            chain,
            chain_id,
            name,
            symbol: None,
            description: None,
            is_proxy: false,
            implementation_address: None,
            source_code,
            abi,
            tvl: None,
            transaction_count: None,
            last_activity_at: None,
            data_source,
        }
    }

    /// Builder pattern methods
    pub fn with_symbol(mut self, symbol: String) -> Self {
        self.symbol = Some(symbol);
        self
    }

    pub fn with_description(mut self, description: String) -> Self {
        self.description = Some(description);
        self
    }

    pub fn with_proxy(mut self, is_proxy: bool, implementation: Option<String>) -> Self {
        self.is_proxy = is_proxy;
        self.implementation_address = implementation;
        self
    }

    pub fn with_tvl(mut self, tvl: f64) -> Self {
        self.tvl = Some(tvl);
        self
    }

    pub fn with_transaction_count(mut self, count: i64) -> Self {
        self.transaction_count = Some(count);
        self
    }

    pub fn with_last_activity(mut self, timestamp: i64) -> Self {
        self.last_activity_at = Some(timestamp);
        self
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_data_source_conversion() {
        assert_eq!(DataSource::DefiLlama.as_str(), "defillama");
        assert_eq!(DataSource::CoinGecko.as_str(), "coingecko");
        assert_eq!(DataSource::Etherscan.as_str(), "etherscan");
        assert_eq!(DataSource::Manual.as_str(), "manual");

        assert_eq!(
            DataSource::from_str("defillama"),
            Some(DataSource::DefiLlama)
        );
        assert_eq!(
            DataSource::from_str("COINGECKO"),
            Some(DataSource::CoinGecko)
        );
        assert_eq!(DataSource::from_str("unknown"), None);
    }

    #[test]
    fn test_data_source_display() {
        assert_eq!(format!("{}", DataSource::DefiLlama), "defillama");
        assert_eq!(format!("{}", DataSource::CoinGecko), "coingecko");
    }

    #[test]
    fn test_contract_builder() {
        let contract = Contract::new(
            "0x123".to_string(),
            "ethereum".to_string(),
            1,
            "Test Contract".to_string(),
            "contract code".to_string(),
            "[]".to_string(),
            DataSource::Manual,
        )
        .with_symbol("TEST".to_string())
        .with_tvl(1000000.0)
        .with_transaction_count(500);

        assert_eq!(contract.address, "0x123");
        assert_eq!(contract.name, "Test Contract");
        assert_eq!(contract.symbol, Some("TEST".to_string()));
        assert_eq!(contract.tvl, Some(1000000.0));
        assert_eq!(contract.transaction_count, Some(500));
    }

    #[test]
    fn test_enriched_contract_conversion() {
        let enriched = EnrichedContract {
            metadata: ContractMetadata {
                address: "0x123".to_string(),
                chain_id: 1,
                chain: "ethereum".to_string(),
                name: "Test".to_string(),
                symbol: Some("TEST".to_string()),
                description: None,
            },
            code: ContractCode {
                source_code: "code".to_string(),
                abi: "[]".to_string(),
                is_proxy: false,
                implementation_address: None,
            },
            metrics: ContractMetrics {
                tvl: Some(1000.0),
                transaction_count: Some(100),
                last_activity_at: Some(1234567890),
            },
        };

        let contract = enriched.into_contract(DataSource::DefiLlama);

        assert_eq!(contract.address, "0x123");
        assert_eq!(contract.name, "Test");
        assert_eq!(contract.tvl, Some(1000.0));
        assert_eq!(contract.data_source, DataSource::DefiLlama);
    }
}
