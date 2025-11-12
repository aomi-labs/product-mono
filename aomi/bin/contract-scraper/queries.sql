-- Contract Scraper Database Queries
-- Use these queries to verify and inspect scraped contracts

-- =============================================================================
-- 1. Check total number of contracts
-- =============================================================================
SELECT COUNT(*) as total_contracts FROM contracts;

-- =============================================================================
-- 2. Show recent contracts with key details
-- =============================================================================
SELECT
    name,
    symbol,
    chain,
    address,
    tvl,
    transaction_count,
    data_source,
    to_timestamp(created_at) as created,
    to_timestamp(updated_at) as updated
FROM contracts
ORDER BY created_at DESC
LIMIT 20;

-- =============================================================================
-- 3. Count contracts by chain
-- =============================================================================
SELECT
    chain,
    COUNT(*) as contract_count,
    SUM(CASE WHEN tvl IS NOT NULL THEN 1 ELSE 0 END) as with_tvl,
    SUM(CASE WHEN transaction_count IS NOT NULL THEN 1 ELSE 0 END) as with_tx_count
FROM contracts
GROUP BY chain
ORDER BY contract_count DESC;

-- =============================================================================
-- 4. Show top contracts by TVL
-- =============================================================================
SELECT
    name,
    symbol,
    chain,
    address,
    ROUND(tvl::numeric, 2) as tvl,
    transaction_count
FROM contracts
WHERE tvl IS NOT NULL
ORDER BY tvl DESC
LIMIT 10;

-- =============================================================================
-- 5. Check contracts scraped in last hour
-- =============================================================================
SELECT
    name,
    chain,
    address,
    tvl,
    to_timestamp(created_at) as created
FROM contracts
WHERE created_at > EXTRACT(EPOCH FROM NOW() - INTERVAL '1 hour')::BIGINT
ORDER BY created_at DESC;

-- =============================================================================
-- 6. Check contracts scraped in last 24 hours
-- =============================================================================
SELECT
    name,
    symbol,
    chain,
    address,
    ROUND(tvl::numeric, 2) as tvl,
    to_timestamp(created_at) as created
FROM contracts
WHERE created_at > EXTRACT(EPOCH FROM NOW() - INTERVAL '24 hours')::BIGINT
ORDER BY created_at DESC;

-- =============================================================================
-- 7. Show proxy contracts
-- =============================================================================
SELECT
    name,
    chain,
    address,
    implementation_address,
    tvl
FROM contracts
WHERE is_proxy = true
ORDER BY tvl DESC NULLS LAST
LIMIT 20;

-- =============================================================================
-- 8. Show contracts by data source
-- =============================================================================
SELECT
    data_source,
    COUNT(*) as count,
    ROUND(AVG(tvl)::numeric, 2) as avg_tvl
FROM contracts
GROUP BY data_source
ORDER BY count DESC;

-- =============================================================================
-- 9. Check for duplicates across chains (same address)
-- =============================================================================
SELECT
    address,
    COUNT(*) as chain_count,
    STRING_AGG(chain, ', ') as chains
FROM contracts
GROUP BY address
HAVING COUNT(*) > 1
ORDER BY chain_count DESC;

-- =============================================================================
-- 10. Get contracts needing update (older than 7 days)
-- =============================================================================
SELECT
    name,
    chain,
    address,
    to_timestamp(updated_at) as last_updated,
    EXTRACT(DAY FROM NOW() - to_timestamp(updated_at)) as days_old
FROM contracts
WHERE updated_at < EXTRACT(EPOCH FROM NOW() - INTERVAL '7 days')::BIGINT
ORDER BY updated_at ASC
LIMIT 20;
