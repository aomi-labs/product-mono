use std::{fmt, str::FromStr, sync::Mutex};

use alloy::rpc::types::{TransactionInput, TransactionRequest};
use alloy_primitives::{Address, Bytes};
use alloy_provider::Provider;
use alloy_sol_types::{SolCall, sol};
use anyhow::{Context, Result, anyhow};
use aomi_tools::clients::CastClient;
use async_trait::async_trait;

/// Default RPC network key used by eval assertions.
pub const DEFAULT_ASSERTION_NETWORK: &str = "ethereum";
/// Helper constant for converting ETH to wei in deterministic checks.
pub const WEI_PER_ETH: u128 = 1_000_000_000_000_000_000u128;

/// Asset descriptor for balance assertions.
#[derive(Debug, Clone)]
pub enum BalanceAsset {
    Native {
        symbol: String,
        decimals: u8,
    },
    Erc20 {
        address: Address,
        symbol: String,
        decimals: u8,
    },
    AaveV3AToken {
        pool: Address,
        underlying: Address,
        symbol: String,
        decimals: u8,
    },
}

impl BalanceAsset {
    pub fn eth() -> Self {
        Self::Native {
            symbol: "ETH".to_string(),
            decimals: 18,
        }
    }

    pub fn native(symbol: impl Into<String>, decimals: u8) -> Self {
        Self::Native {
            symbol: symbol.into(),
            decimals,
        }
    }

    pub fn erc20(
        symbol: impl Into<String>,
        address: impl AsRef<str>,
        decimals: u8,
    ) -> Result<Self> {
        let parsed = Address::from_str(address.as_ref())
            .map_err(|err| anyhow!("invalid token address '{}': {}", address.as_ref(), err))?;
        Ok(Self::Erc20 {
            address: parsed,
            symbol: symbol.into(),
            decimals,
        })
    }

    pub fn aave_v3_atoken(
        symbol: impl Into<String>,
        pool: impl AsRef<str>,
        underlying: impl AsRef<str>,
        decimals: u8,
    ) -> Result<Self> {
        let pool = Address::from_str(pool.as_ref())
            .map_err(|err| anyhow!("invalid pool address '{}': {}", pool.as_ref(), err))?;
        let underlying = Address::from_str(underlying.as_ref()).map_err(|err| {
            anyhow!(
                "invalid underlying address '{}': {}",
                underlying.as_ref(),
                err
            )
        })?;
        Ok(Self::AaveV3AToken {
            pool,
            underlying,
            symbol: symbol.into(),
            decimals,
        })
    }

    pub fn usdt(address: impl AsRef<str>) -> Result<Self> {
        Self::erc20("USDT", address, 6)
    }

    pub fn usdc(address: impl AsRef<str>) -> Result<Self> {
        Self::erc20("USDC", address, 6)
    }

    pub fn symbol(&self) -> &str {
        match self {
            BalanceAsset::Native { symbol, .. } => symbol,
            BalanceAsset::Erc20 { symbol, .. } => symbol,
            BalanceAsset::AaveV3AToken { symbol, .. } => symbol,
        }
    }

    pub fn decimals(&self) -> u8 {
        match self {
            BalanceAsset::Native { decimals, .. } => *decimals,
            BalanceAsset::Erc20 { decimals, .. } => *decimals,
            BalanceAsset::AaveV3AToken { decimals, .. } => *decimals,
        }
    }

    pub fn address(&self) -> Option<Address> {
        match self {
            BalanceAsset::Native { .. } => None,
            BalanceAsset::Erc20 { address, .. } => Some(*address),
            BalanceAsset::AaveV3AToken { .. } => None,
        }
    }

    pub fn is_native(&self) -> bool {
        matches!(self, BalanceAsset::Native { .. })
    }
}

impl fmt::Display for BalanceAsset {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            BalanceAsset::Native { symbol, .. } => write!(f, "{symbol}"),
            BalanceAsset::Erc20 {
                symbol, address, ..
            } => {
                write!(f, "{} ({:#x})", symbol, address)
            }
            BalanceAsset::AaveV3AToken {
                symbol,
                pool,
                underlying,
                ..
            } => write!(
                f,
                "{} (pool {:#x}, underlying {:#x})",
                symbol, pool, underlying
            ),
        }
    }
}

#[derive(Debug, Clone)]
pub struct BalanceChange {
    pub holder: String,
    pub asset: BalanceAsset,
    pub expected_delta_units: i128,
    pub tolerance_units: u128,
    pub label: String,
}

impl BalanceChange {
    pub fn new(
        holder: impl Into<String>,
        asset: BalanceAsset,
        expected_delta_units: i128,
        tolerance_units: u128,
        label: impl Into<String>,
    ) -> Self {
        Self {
            holder: holder.into(),
            asset,
            expected_delta_units,
            tolerance_units,
            label: label.into(),
        }
    }

    pub fn eth_delta(
        holder: impl Into<String>,
        expected_delta_wei: i128,
        tolerance_wei: u128,
        label: impl Into<String>,
    ) -> Self {
        Self::new(
            holder,
            BalanceAsset::eth(),
            expected_delta_wei,
            tolerance_wei,
            label,
        )
    }

    pub fn eth_increase(
        holder: impl Into<String>,
        amount_eth: u64,
        label: impl Into<String>,
    ) -> Result<Self> {
        let base = i128::from(amount_eth);
        let delta = base
            .checked_mul(WEI_PER_ETH as i128)
            .ok_or_else(|| anyhow!("eth amount {amount_eth} is too large for wei conversion"))?;
        Ok(Self::eth_delta(holder, delta, 0, label))
    }

    pub fn asset_delta(
        holder: impl Into<String>,
        asset: BalanceAsset,
        expected_delta_units: i128,
        tolerance_units: u128,
        label: impl Into<String>,
    ) -> Self {
        Self::new(holder, asset, expected_delta_units, tolerance_units, label)
    }

    pub fn token_increase(
        holder: impl Into<String>,
        asset: BalanceAsset,
        amount_units: u128,
        label: impl Into<String>,
    ) -> Result<Self> {
        let delta = i128::try_from(amount_units)
            .map_err(|_| anyhow!("token amount {amount_units} overflows i128"))?;
        Ok(Self::asset_delta(holder, asset, delta, 0, label))
    }
}

#[derive(Debug, Clone)]
pub struct BalanceCheck {
    pub holder: String,
    pub asset: BalanceAsset,
    pub expected_units: u128,
    pub tolerance_units: u128,
    pub label: String,
}

impl BalanceCheck {
    pub fn new(
        holder: impl Into<String>,
        asset: BalanceAsset,
        expected_units: u128,
        tolerance_units: u128,
        label: impl Into<String>,
    ) -> Self {
        Self {
            holder: holder.into(),
            asset,
            expected_units,
            tolerance_units,
            label: label.into(),
        }
    }

    pub fn eth_equals(
        holder: impl Into<String>,
        amount_eth: u64,
        tolerance_wei: u128,
        label: impl Into<String>,
    ) -> Result<Self> {
        let base = u128::from(amount_eth);
        let expected_units = base
            .checked_mul(WEI_PER_ETH)
            .ok_or_else(|| anyhow!("eth amount {amount_eth} is too large for wei conversion"))?;
        Ok(Self::new(
            holder,
            BalanceAsset::eth(),
            expected_units,
            tolerance_wei,
            label,
        ))
    }

    pub fn asset_equals(
        holder: impl Into<String>,
        asset: BalanceAsset,
        expected_units: u128,
        tolerance_units: u128,
        label: impl Into<String>,
    ) -> Self {
        Self::new(holder, asset, expected_units, tolerance_units, label)
    }
}

#[derive(Debug, Clone)]
pub enum AssertionPlan {
    BalanceChange(BalanceChange),
    BalanceCheck(BalanceCheck),
    BalanceAtLeast {
        holder: String,
        asset: BalanceAsset,
        min_units: u128,
        label: String,
    },
    BalanceDeltaAtLeast(BalanceChange),
    BalanceDeltaAtMost(BalanceChange),
}

impl AssertionPlan {
    pub fn into_assertion(self, test_id: usize) -> Result<Box<dyn Assertion>> {
        match self {
            AssertionPlan::BalanceChange(change) => {
                Ok(Box::new(BalanceDeltaAssertion::new(test_id, change)?))
            }
            AssertionPlan::BalanceCheck(check) => {
                Ok(Box::new(BalanceEqualsAssertion::new(test_id, check)?))
            }
            AssertionPlan::BalanceAtLeast {
                holder,
                asset,
                min_units,
                label,
            } => Ok(Box::new(BalanceAtLeastAssertion::new(
                test_id, holder, asset, min_units, label,
            )?)),
            AssertionPlan::BalanceDeltaAtLeast(change) => Ok(Box::new(
                BalanceDeltaAtLeastAssertion::new(test_id, change)?,
            )),
            AssertionPlan::BalanceDeltaAtMost(change) => {
                Ok(Box::new(BalanceDeltaAtMostAssertion::new(test_id, change)?))
            }
        }
    }
}

#[derive(Debug, Clone)]
pub struct AssertionResult {
    pub test_id: usize,
    pub label: String,
    pub passed: bool,
    pub detail: String,
}

#[async_trait]
pub trait Assertion: Send + Sync {
    fn test_id(&self) -> usize;
    fn label(&self) -> &str;
    async fn snapshot(&self, client: &CastClient) -> Result<()>;
    async fn verify(&self, client: &CastClient) -> Result<AssertionResult>;
}

struct BalanceDeltaAssertion {
    test_id: usize,
    holder: Address,
    asset: BalanceAsset,
    expected_delta_units: i128,
    tolerance_units: u128,
    label: String,
    baseline_units: Mutex<Option<u128>>,
}

impl BalanceDeltaAssertion {
    fn new(test_id: usize, change: BalanceChange) -> Result<Self> {
        let holder = Address::from_str(change.holder.as_str())
            .map_err(|err| anyhow!("invalid address '{}': {}", change.holder, err))?;

        Ok(Self {
            test_id,
            holder,
            asset: change.asset,
            expected_delta_units: change.expected_delta_units,
            tolerance_units: change.tolerance_units,
            label: change.label,
            baseline_units: Mutex::new(None),
        })
    }
}

#[async_trait]
impl Assertion for BalanceDeltaAssertion {
    fn test_id(&self) -> usize {
        self.test_id
    }

    fn label(&self) -> &str {
        &self.label
    }

    async fn snapshot(&self, client: &CastClient) -> Result<()> {
        let balance = load_balance(client, &self.asset, self.holder).await?;
        let mut guard = self.baseline_units.lock().unwrap();
        *guard = Some(balance);
        Ok(())
    }

    async fn verify(&self, client: &CastClient) -> Result<AssertionResult> {
        let before = {
            let guard = self.baseline_units.lock().unwrap();
            guard.as_ref().cloned().ok_or_else(|| {
                anyhow!(
                    "missing baseline for deterministic assertion '{}'",
                    self.label
                )
            })?
        };
        let after = load_balance(client, &self.asset, self.holder)
            .await
            .context("failed to fetch post-run balance")?;

        let before_i128 =
            i128::try_from(before).map_err(|_| anyhow!("pre-run balance does not fit in i128"))?;
        let after_i128 =
            i128::try_from(after).map_err(|_| anyhow!("post-run balance does not fit in i128"))?;

        let actual_delta = after_i128 - before_i128;
        let tolerance_i128 = i128::try_from(self.tolerance_units)
            .map_err(|_| anyhow!("tolerance does not fit in i128"))?;
        let lower = self.expected_delta_units - tolerance_i128;
        let upper = self.expected_delta_units + tolerance_i128;
        let passed = actual_delta >= lower && actual_delta <= upper;

        let detail = format!(
            "{} (before: {}, after: {}, delta: {}, expected: {} ± {})",
            self.label,
            format_units(before, &self.asset),
            format_units(after, &self.asset),
            format_delta(actual_delta, &self.asset),
            format_delta(self.expected_delta_units, &self.asset),
            format_units(self.tolerance_units, &self.asset),
        );

        Ok(AssertionResult {
            test_id: self.test_id,
            label: self.label.clone(),
            passed,
            detail,
        })
    }
}

struct BalanceEqualsAssertion {
    test_id: usize,
    holder: Address,
    asset: BalanceAsset,
    expected_units: u128,
    tolerance_units: u128,
    label: String,
}

impl BalanceEqualsAssertion {
    fn new(test_id: usize, check: BalanceCheck) -> Result<Self> {
        let holder = Address::from_str(check.holder.as_str())
            .map_err(|err| anyhow!("invalid address '{}': {}", check.holder, err))?;

        Ok(Self {
            test_id,
            holder,
            asset: check.asset,
            expected_units: check.expected_units,
            tolerance_units: check.tolerance_units,
            label: check.label,
        })
    }
}

struct BalanceDeltaAtLeastAssertion {
    test_id: usize,
    holder: Address,
    asset: BalanceAsset,
    min_delta_units: i128,
    label: String,
    baseline_units: Mutex<Option<u128>>,
}

impl BalanceDeltaAtLeastAssertion {
    fn new(test_id: usize, change: BalanceChange) -> Result<Self> {
        let holder = Address::from_str(change.holder.as_str())
            .map_err(|err| anyhow!("invalid address '{}': {}", change.holder, err))?;

        Ok(Self {
            test_id,
            holder,
            asset: change.asset,
            min_delta_units: change.expected_delta_units,
            label: change.label,
            baseline_units: Mutex::new(None),
        })
    }
}

#[async_trait]
impl Assertion for BalanceDeltaAtLeastAssertion {
    fn test_id(&self) -> usize {
        self.test_id
    }

    fn label(&self) -> &str {
        &self.label
    }

    async fn snapshot(&self, client: &CastClient) -> Result<()> {
        let balance = load_balance(client, &self.asset, self.holder).await?;
        let mut guard = self.baseline_units.lock().unwrap();
        *guard = Some(balance);
        Ok(())
    }

    async fn verify(&self, client: &CastClient) -> Result<AssertionResult> {
        let before = {
            let guard = self.baseline_units.lock().unwrap();
            guard.as_ref().cloned().ok_or_else(|| {
                anyhow!(
                    "missing baseline for deterministic assertion '{}'",
                    self.label
                )
            })?
        };
        let after = load_balance(client, &self.asset, self.holder)
            .await
            .context("failed to fetch post-run balance")?;

        let before_i128 =
            i128::try_from(before).map_err(|_| anyhow!("pre-run balance does not fit in i128"))?;
        let after_i128 =
            i128::try_from(after).map_err(|_| anyhow!("post-run balance does not fit in i128"))?;

        let actual_delta = after_i128 - before_i128;
        let passed = actual_delta >= self.min_delta_units;

        let detail = format!(
            "{} (before: {}, after: {}, delta: {}, required: ≥ {})",
            self.label,
            format_units(before, &self.asset),
            format_units(after, &self.asset),
            format_delta(actual_delta, &self.asset),
            format_delta(self.min_delta_units, &self.asset),
        );

        Ok(AssertionResult {
            test_id: self.test_id,
            label: self.label.clone(),
            passed,
            detail,
        })
    }
}

struct BalanceDeltaAtMostAssertion {
    test_id: usize,
    holder: Address,
    asset: BalanceAsset,
    max_delta_units: i128,
    label: String,
    baseline_units: Mutex<Option<u128>>,
}

impl BalanceDeltaAtMostAssertion {
    fn new(test_id: usize, change: BalanceChange) -> Result<Self> {
        let holder = Address::from_str(change.holder.as_str())
            .map_err(|err| anyhow!("invalid address '{}': {}", change.holder, err))?;

        Ok(Self {
            test_id,
            holder,
            asset: change.asset,
            max_delta_units: change.expected_delta_units,
            label: change.label,
            baseline_units: Mutex::new(None),
        })
    }
}

#[async_trait]
impl Assertion for BalanceDeltaAtMostAssertion {
    fn test_id(&self) -> usize {
        self.test_id
    }

    fn label(&self) -> &str {
        &self.label
    }

    async fn snapshot(&self, client: &CastClient) -> Result<()> {
        let balance = load_balance(client, &self.asset, self.holder).await?;
        let mut guard = self.baseline_units.lock().unwrap();
        *guard = Some(balance);
        Ok(())
    }

    async fn verify(&self, client: &CastClient) -> Result<AssertionResult> {
        let before = {
            let guard = self.baseline_units.lock().unwrap();
            guard.as_ref().cloned().ok_or_else(|| {
                anyhow!(
                    "missing baseline for deterministic assertion '{}'",
                    self.label
                )
            })?
        };
        let after = load_balance(client, &self.asset, self.holder)
            .await
            .context("failed to fetch post-run balance")?;

        let before_i128 =
            i128::try_from(before).map_err(|_| anyhow!("pre-run balance does not fit in i128"))?;
        let after_i128 =
            i128::try_from(after).map_err(|_| anyhow!("post-run balance does not fit in i128"))?;

        let actual_delta = after_i128 - before_i128;
        let passed = actual_delta <= self.max_delta_units;

        let detail = format!(
            "{} (before: {}, after: {}, delta: {}, required: ≤ {})",
            self.label,
            format_units(before, &self.asset),
            format_units(after, &self.asset),
            format_delta(actual_delta, &self.asset),
            format_delta(self.max_delta_units, &self.asset),
        );

        Ok(AssertionResult {
            test_id: self.test_id,
            label: self.label.clone(),
            passed,
            detail,
        })
    }
}

#[async_trait]
impl Assertion for BalanceEqualsAssertion {
    fn test_id(&self) -> usize {
        self.test_id
    }

    fn label(&self) -> &str {
        &self.label
    }

    async fn snapshot(&self, _client: &CastClient) -> Result<()> {
        // No baseline needed for absolute balance checks.
        Ok(())
    }

    async fn verify(&self, client: &CastClient) -> Result<AssertionResult> {
        let balance = load_balance(client, &self.asset, self.holder)
            .await
            .context("failed to fetch balance for equality assertion")?;

        let balance_i128 =
            i128::try_from(balance).map_err(|_| anyhow!("balance does not fit in i128"))?;
        let expected_i128 = i128::try_from(self.expected_units)
            .map_err(|_| anyhow!("expected balance does not fit in i128"))?;
        let tolerance_i128 = i128::try_from(self.tolerance_units)
            .map_err(|_| anyhow!("tolerance does not fit in i128"))?;

        let lower = expected_i128 - tolerance_i128;
        let upper = expected_i128 + tolerance_i128;
        let passed = balance_i128 >= lower && balance_i128 <= upper;

        let detail = format!(
            "{} (actual: {}, expected: {} ± {})",
            self.label,
            format_units(balance, &self.asset),
            format_units(self.expected_units, &self.asset),
            format_units(self.tolerance_units, &self.asset),
        );

        Ok(AssertionResult {
            test_id: self.test_id,
            label: self.label.clone(),
            passed,
            detail,
        })
    }
}

struct BalanceAtLeastAssertion {
    test_id: usize,
    holder: Address,
    asset: BalanceAsset,
    min_units: u128,
    label: String,
}

impl BalanceAtLeastAssertion {
    fn new(
        test_id: usize,
        holder: impl AsRef<str>,
        asset: BalanceAsset,
        min_units: u128,
        label: impl Into<String>,
    ) -> Result<Self> {
        let holder = Address::from_str(holder.as_ref())
            .map_err(|err| anyhow!("invalid address '{}': {}", holder.as_ref(), err))?;

        Ok(Self {
            test_id,
            holder,
            asset,
            min_units,
            label: label.into(),
        })
    }
}

#[async_trait]
impl Assertion for BalanceAtLeastAssertion {
    fn test_id(&self) -> usize {
        self.test_id
    }

    fn label(&self) -> &str {
        &self.label
    }

    async fn snapshot(&self, _client: &CastClient) -> Result<()> {
        Ok(())
    }

    async fn verify(&self, client: &CastClient) -> Result<AssertionResult> {
        let balance = load_balance(client, &self.asset, self.holder)
            .await
            .context("failed to fetch balance for minimum assertion")?;

        let passed = balance >= self.min_units;
        let detail = format!(
            "{} (actual: {}, required: ≥ {})",
            self.label,
            format_units(balance, &self.asset),
            format_units(self.min_units, &self.asset)
        );

        Ok(AssertionResult {
            test_id: self.test_id,
            label: self.label.clone(),
            passed,
            detail,
        })
    }
}

async fn load_balance(client: &CastClient, asset: &BalanceAsset, holder: Address) -> Result<u128> {
    match asset {
        BalanceAsset::Native { .. } => load_native_balance(client, holder).await,
        BalanceAsset::Erc20 { address, .. } => load_token_balance(client, *address, holder).await,
        BalanceAsset::AaveV3AToken {
            pool, underlying, ..
        } => {
            let atoken = resolve_aave_v3_atoken_address(client, *pool, *underlying).await?;
            load_token_balance(client, atoken, holder).await
        }
    }
}

async fn resolve_aave_v3_atoken_address(
    client: &CastClient,
    pool: Address,
    underlying: Address,
) -> Result<Address> {
    sol! {
        function getReserveData(address asset)
            returns (
                uint256 configuration,
                uint128 liquidityIndex,
                uint128 currentLiquidityRate,
                uint128 variableBorrowIndex,
                uint128 currentVariableBorrowRate,
                uint128 currentStableBorrowRate,
                uint40 lastUpdateTimestamp,
                uint16 id,
                address aTokenAddress,
                address stableDebtTokenAddress,
                address variableDebtTokenAddress,
                address interestRateStrategyAddress,
                uint128 accruedToTreasury,
                uint128 unbacked,
                uint128 isolationModeTotalDebt
            );
    }

    let calldata: Bytes = getReserveDataCall { asset: underlying }.abi_encode().into();
    let tx = TransactionRequest::default()
        .to(pool)
        .input(TransactionInput::new(calldata))
        .with_input_and_data();

    let raw = Provider::call(&client.provider, tx.into())
        .await
        .context("failed to call Aave V3 getReserveData")?;
    let decoded = getReserveDataCall::abi_decode_returns(raw.as_ref())
        .context("failed to decode Aave V3 reserve data")?;

    Ok(decoded.aTokenAddress)
}

async fn load_native_balance(client: &CastClient, holder: Address) -> Result<u128> {
    let balance = client
        .provider
        .get_balance(holder)
        .await
        .context("failed to fetch balance from provider")?;

    balance
        .to_string()
        .parse::<u128>()
        .map_err(|err| anyhow!("failed to parse balance '{}' into u128: {}", balance, err))
}

async fn load_token_balance(client: &CastClient, token: Address, holder: Address) -> Result<u128> {
    use tokio::time::{Duration, sleep};

    sol! {
        #[allow(non_camel_case_types)]
        function balanceOf(address owner) returns (uint256);
    }

    let calldata: Bytes = balanceOfCall { owner: holder }.abi_encode().into();

    // Retry logic for fork initialization race conditions
    let mut last_error = None;
    for attempt in 0..3 {
        if attempt > 0 {
            sleep(Duration::from_millis(500)).await;
        }

        let tx = TransactionRequest::default()
            .to(token)
            .input(TransactionInput::new(calldata.clone()))
            .with_input_and_data();

        let raw = match client.provider.call(tx.into()).await {
            Ok(r) => r,
            Err(e) => {
                last_error = Some(anyhow::Error::from(e));
                continue;
            }
        };

        let decoded = match balanceOfCall::abi_decode_returns(raw.as_ref()) {
            Ok(d) => d,
            Err(e) => {
                last_error = Some(anyhow::Error::msg(format!(
                    "failed to decode ERC20 balanceOf return payload for token {} holder {} (got {} bytes: 0x{}): {}",
                    token,
                    holder,
                    raw.len(),
                    hex::encode(&raw),
                    e
                )));
                continue;
            }
        };

        return decoded
            .to_string()
            .parse::<u128>()
            .context("failed to parse ERC20 balance into u128");
    }

    Err(last_error.unwrap_or_else(|| anyhow!("failed to load token balance after retries")))
}

fn format_units(amount: u128, asset: &BalanceAsset) -> String {
    let symbol = asset.symbol();
    if asset.decimals() == 0 {
        return format!("{amount} {symbol}");
    }

    let divisor = decimals_divisor(asset.decimals());
    let Some(divisor) = divisor else {
        return format!(
            "{amount} {symbol} (base units, {} decimals)",
            asset.decimals()
        );
    };

    let whole = amount / divisor;
    let remainder = amount % divisor;

    if remainder == 0 {
        format!("{whole} {symbol}")
    } else {
        let width = asset.decimals() as usize;
        let mut fractional = format!("{remainder:0width$}", width = width);
        while fractional.ends_with('0') {
            fractional.pop();
        }
        format!("{whole}.{fractional} {symbol}")
    }
}

fn decimals_divisor(decimals: u8) -> Option<u128> {
    10u128.checked_pow(u32::from(decimals))
}

fn format_delta(delta: i128, asset: &BalanceAsset) -> String {
    if delta < 0 {
        let magnitude = delta.unsigned_abs();
        format!("-{}", format_units(magnitude, asset))
    } else {
        format!("+{}", format_units(delta as u128, asset))
    }
}
