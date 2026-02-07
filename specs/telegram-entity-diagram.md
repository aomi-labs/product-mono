# Telegram Bot — Component Diagram

```mermaid
graph TB
    User((User))

    subgraph "Telegram"
        TG_API[Telegram API]
    end

    subgraph "bin/telegram"
        Bot[TelegramBot - teloxide Dispatcher]
        Cmds[Commands - start, connect, wallet, namespace, model, help]
        Handlers[Handlers - DM + Group routing, poll loop]
        Send[Send - markdown to HTML, chunk 4000]
    end

    subgraph "crates/bot-core"
        SessionKey[SessionKeyBuilder]
        Policy[Policy - DM and Group access control]
        WalletSvc[DbWalletConnectService - EIP-191]
        Poller[ResponsePoller - 60s timeout]
    end

    subgraph "crates/backend"
        SM[SessionManager - sessions + backends]
        NS[Namespaces - Default, L2b, Forge, Polymarket, X, Delta]
    end

    subgraph "crates/core"
        LLM[CoreApp - LLM streaming + tools]
    end

    subgraph "mini-app Next.js"
        Connect[connect-wallet - RainbowKit UI]
        BindAPI[api/wallet/bind]
        TxAPI[api/wallet/tx]
        SignPage[sign page]
    end

    DB[(PostgreSQL - users, sessions, signup_challenges)]

    User -->|message| TG_API
    TG_API -->|long-poll| Bot

    Bot -->|slash command| Cmds
    Bot -->|text msg| Handlers

    Handlers --> SessionKey
    Handlers --> Policy
    Handlers --> WalletSvc
    Handlers --> Send
    Cmds --> WalletSvc
    Cmds --> SessionKey

    Handlers --> SM
    Cmds --> SM
    Poller --> SM
    SM --> NS
    NS --> LLM

    WalletSvc --> DB
    BindAPI --> DB

    Cmds -.->|WebApp button| Connect
    Handlers -.->|wallet_tx_request sign button| SignPage

    Connect -->|POST| BindAPI
    BindAPI -->|confirmation msg| TG_API
    SignPage -->|GET/PUT| TxAPI
    Handlers -->|POST create tx| TxAPI

    Send -->|formatted chunks| TG_API
    TG_API -->|display| User
```

## Session key formats

| Chat Type | Format | Example |
|-----------|--------|---------|
| DM | `telegram:dm:{user_id}` | `telegram:dm:123456789` |
| Group | `telegram:group:{chat_id}` | `telegram:group:-100123456789` |
| Thread | `telegram:group:{chat_id}:thread:{thread_id}` | `telegram:group:-100123:thread:42` |
