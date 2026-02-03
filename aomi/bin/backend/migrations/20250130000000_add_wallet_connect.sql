-- Wallet connect tables for binding chat sessions to Ethereum wallets

-- Store pending challenges for signature verification
CREATE TABLE IF NOT EXISTS wallet_challenges (
    session_key VARCHAR(255) PRIMARY KEY,
    nonce VARCHAR(64) NOT NULL,
    created_at TIMESTAMP WITH TIME ZONE DEFAULT NOW()
);

-- Store verified wallet bindings
CREATE TABLE IF NOT EXISTS user_wallets (
    session_key VARCHAR(255) PRIMARY KEY,
    wallet_address VARCHAR(42) NOT NULL,
    verified_at TIMESTAMP WITH TIME ZONE DEFAULT NOW(),
    created_at TIMESTAMP WITH TIME ZONE DEFAULT NOW()
);

-- Index for looking up sessions by wallet
CREATE INDEX IF NOT EXISTS idx_user_wallets_address ON user_wallets(wallet_address);

-- Clean up stale challenges older than 10 minutes (optional, can be done via cron)
-- DELETE FROM wallet_challenges WHERE created_at < NOW() - INTERVAL '10 minutes';
