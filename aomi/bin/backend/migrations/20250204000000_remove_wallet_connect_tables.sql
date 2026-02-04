-- Remove deprecated wallet connect tables (wallet_challenges, user_wallets)

DROP INDEX IF EXISTS idx_user_wallets_address;
DROP TABLE IF EXISTS wallet_challenges;
DROP TABLE IF EXISTS user_wallets;
