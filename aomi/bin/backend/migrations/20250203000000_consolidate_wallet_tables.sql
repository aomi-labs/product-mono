-- Consolidate wallet tables: rename wallet_challenges, drop user_wallets
-- Note: users.public_key IS the wallet address, no separate column needed

-- 1. Rename wallet_challenges to signup_challenges
ALTER TABLE IF EXISTS wallet_challenges RENAME TO signup_challenges;

-- 2. Rename session_key to session_id for consistency with sessions table
ALTER TABLE signup_challenges RENAME COLUMN session_key TO session_id;

-- 3. Add foreign key constraint for referential integrity
ALTER TABLE signup_challenges 
    ADD CONSTRAINT fk_signup_challenges_session 
    FOREIGN KEY (session_id) REFERENCES sessions(id) ON DELETE CASCADE;

-- 4. Drop user_wallets table (wallet is stored as users.public_key)
DROP TABLE IF EXISTS user_wallets;
