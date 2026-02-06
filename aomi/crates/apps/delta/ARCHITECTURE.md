# Delta Network Architecture

## Network Topology

```
┌─────────────────────────────────────────────────────────────────────────┐
│                          BASE LAYER (Consensus)                         │
│  ┌──────────┐  ┌──────────┐  ┌──────────┐  ┌──────────┐                │
│  │Validator │  │Validator │  │Validator │  │Validator │  ...           │
│  │    1     │  │    2     │  │    3     │  │    4     │                │
│  └────┬─────┘  └────┬─────┘  └────┬─────┘  └────┬─────┘                │
│       │             │             │             │                       │
│       └─────────────┴──────┬──────┴─────────────┘                       │
│                            │                                            │
│              ┌─────────────┴─────────────┐                              │
│              │    FINALIZED STATE        │                              │
│              │  (SDL Proofs + Balances)  │                              │
│              └─────────────┬─────────────┘                              │
└────────────────────────────┼────────────────────────────────────────────┘
                             │
        ┌────────────────────┼────────────────────┐
        │                    │                    │
        ▼                    ▼                    ▼
┌───────────────┐   ┌───────────────┐   ┌───────────────┐
│   SHARD 1     │   │   SHARD 9     │   │   SHARD N     │
│   (DEX?)      │   │  (AOMI RFQ)   │   │   (Other)     │
├───────────────┤   ├───────────────┤   ├───────────────┤
│ Domain        │   │ Domain        │   │ Domain        │
│ Operator      │   │ Operator      │   │ Operator      │
│               │   │ (our keypair) │   │               │
├───────────────┤   ├───────────────┤   ├───────────────┤
│ Local Laws    │   │ Local Laws    │   │ Local Laws    │
│ (DEX rules)   │   │ (RFQ rules)   │   │ (Custom)      │
├───────────────┤   ├───────────────┤   ├───────────────┤
│ Vaults        │   │ Vaults        │   │ Vaults        │
│ ┌───────────┐ │   │ ┌───────────┐ │   │ ┌───────────┐ │
│ │User A, 1  │ │   │ │User A, 9  │ │   │ │User A, N  │ │
│ │Balance:100│ │   │ │Balance:50 │ │   │ │Balance:25 │ │
│ └───────────┘ │   │ └───────────┘ │   │ └───────────┘ │
└───────────────┘   └───────────────┘   └───────────────┘
```

## Shards

- **Unlimited shards** - each domain registers its own shard via Domain Agreement
- **State isolation** - vaults on shard 9 are separate from shard 1
- **Same user, multiple shards** - User A can have vaults on many shards
- **Cross-shard credits** - can send TO any shard, can only DEBIT from your shard

## What Validates What

```
┌─────────────────────────────────────────────────────────────────┐
│                     VALIDATION LAYERS                           │
├─────────────────────────────────────────────────────────────────┤
│                                                                 │
│  1. GLOBAL LAWS (Base Layer - ALL transactions)                 │
│     ├─ Signature valid? (ed25519/passkey/multisig)              │
│     ├─ Balance sufficient? (can't debit more than you have)     │
│     ├─ Domain Agreement active? (operator authorized)           │
│     └─ No double-spend? (vault only on one shard)               │
│                                                                 │
│  2. LOCAL LAWS (Domain - YOUR transactions)                     │
│     ├─ RFQ quote valid? (price, size, expiration)               │
│     ├─ Fill conditions met? (taker meets maker terms)           │
│     ├─ Oracle/feed data correct? (price feeds)                  │
│     └─ Custom business logic (KYC, limits, etc.)                │
│                                                                 │
│  3. ZK PROOF (Submitted with SDL)                               │
│     └─ Proves: "All txs in this SDL followed Global+Local Laws" │
│                                                                 │
└─────────────────────────────────────────────────────────────────┘
```

## State Storage

| Layer | What's Stored | Where |
|-------|--------------|-------|
| **Base Layer** | Finalized vault balances, Domain Agreements, SDL proofs | Validators (consensus) |
| **Domain** | Pending transactions, Local state view, Quote book | Domain server (us) |
| **User** | Private keys, Signed verifiables | User wallet |

## Transaction Flow (RFQ Example)

```
1. MAKER creates quote
   └─> Domain validates against Local Laws
   └─> Stored in domain state (not yet on base layer)

2. TAKER fills quote  
   └─> Domain executes fill
   └─> Generates State Diffs (balance changes)
   └─> Aggregates into SDL

3. DOMAIN submits SDL + ZK Proof
   └─> Validators verify proof (Global Laws satisfied)
   └─> Consensus reached
   └─> Balances updated (FINALIZED)

4. SETTLEMENT complete
   └─> Maker received payment
   └─> Taker received asset
   └─> Provably correct (ZK)
```

## Key Insight

Delta is NOT a smart contract chain. It's:
- **Domains** = off-chain execution (fast, flexible)
- **Base Layer** = on-chain settlement (ZK verified, final)
- **Shards** = isolated state spaces (parallel, scalable)

We run the compute, Delta proves it's correct.
