# Telegram Bot — Sequence Diagrams

## 1. DM Message Handling (Happy Path)

```mermaid
sequenceDiagram
    actor User
    participant TG as Telegram API
    participant Bot as TelegramBot<br/>(bot.rs)
    participant Hdl as handlers.rs
    participant Sess as session.rs
    participant WS as DbWalletConnectService
    participant SM as SessionManager
    participant SS as SessionState
    participant Core as CoreApp (LLM)
    participant Send as send.rs

    User->>TG: sends "swap 1 ETH"
    TG->>Bot: Update (long-polling)

    Bot->>Bot: handle_command() → Ok(false)
    Bot->>Hdl: handle_message(bot, msg, session_mgr)

    Hdl->>Hdl: chat.is_private() → true
    Hdl->>Hdl: handle_dm()

    Note over Hdl: Policy check
    Hdl->>Hdl: check DmPolicy (Open/Allowlist/Disabled)

    Hdl->>Sess: dm_session_key(user_id)
    Sess-->>Hdl: "telegram:dm:123456"

    Hdl->>Hdl: process_and_respond()

    Note over Hdl,WS: Load wallet state
    Hdl->>WS: get_bound_wallet(session_key)
    WS-->>Hdl: Some("0xABC...")

    Note over Hdl,SM: Get/create session
    Hdl->>SM: get_session_config(session_key)
    SM-->>Hdl: (Namespace::Default, Selection)
    Hdl->>SM: get_or_create_session(key, auth, None)
    SM-->>Hdl: Arc<Mutex<SessionState>>

    Hdl->>TG: send_chat_action(Typing)

    Note over Hdl,SS: Inject wallet + send input
    Hdl->>SS: sync_user_state(UserState{address, chain_id})
    Hdl->>SS: send_user_input("swap 1 ETH")

    Note over SS,Core: Background task processes
    SS--)Core: CoreApp::process_message()
    Core--)Core: LLM streaming + tool calls

    loop Poll loop (100ms ticks, 60s max)
        Hdl->>SS: sync_state()
        Hdl->>SS: format_session_response(None)
        SS-->>Hdl: SessionResponse{is_processing, messages, events}

        alt is_processing = true
            Note over Hdl: Check system_events for wallet_tx_request
            alt elapsed > 4s since last typing
                Hdl->>TG: send_chat_action(Typing)
            end
            Hdl->>Hdl: drop(lock), sleep(100ms), re-lock
        else is_processing = false
            Note over Hdl: Break poll loop
        end
    end

    Hdl->>Hdl: extract_assistant_text(response)
    Hdl->>Send: format_for_telegram(text)
    Send->>Send: markdown_to_telegram_html()
    Send->>Send: chunk_message(html, 4000)
    Send-->>Hdl: Vec<String> chunks

    loop For each chunk
        Hdl->>TG: send_message(chat_id, chunk, HTML)
    end
    TG->>User: displays response
```

## 2. Group Message Handling

```mermaid
sequenceDiagram
    actor User
    participant TG as Telegram API
    participant Bot as TelegramBot
    participant Hdl as handlers.rs

    User->>TG: "@aomi_bot check ETH price"
    TG->>Bot: Update

    Bot->>Bot: handle_command() → Ok(false)
    Bot->>Hdl: handle_message()

    Hdl->>Hdl: chat.is_group() → true
    Hdl->>Hdl: handle_group()

    alt GroupPolicy::Disabled
        Hdl-->>Bot: Ok(()) [ignored]
    else GroupPolicy::Always
        Note over Hdl: Always process
    else GroupPolicy::Mention
        Hdl->>Hdl: is_bot_mentioned()
        Hdl->>TG: bot.get_me()
        TG-->>Hdl: bot username

        alt Reply to bot's message
            Hdl-->>Hdl: mentioned = true
        else @mention in entities
            Hdl->>Hdl: check MessageEntityKind::Mention
            Hdl-->>Hdl: compare with bot_username
        else Not mentioned
            Hdl-->>Bot: Ok(()) [ignored]
        end
    end

    Hdl->>Hdl: group_session_key(chat_id)
    Note over Hdl: → "telegram:group:-100123"
    Hdl->>Hdl: process_and_respond()
    Note over Hdl: (same flow as DM from here)
```

## 3. Wallet Connect Flow

```mermaid
sequenceDiagram
    actor User
    participant TG as Telegram API
    participant Cmd as commands.rs
    participant MA as Mini App<br/>(connect-wallet.tsx)
    participant API as /api/wallet/bind
    participant DB as PostgreSQL

    User->>TG: /connect
    TG->>Cmd: handle_command("connect")
    Cmd->>Cmd: handle_connect()
    Cmd->>Cmd: get_mini_app_url()
    Note over Cmd: Must be HTTPS

    alt MINI_APP_URL is HTTPS
        Cmd->>Cmd: make_connect_keyboard()
        Cmd->>TG: send_message + InlineKeyboard<br/>[WebApp: "Connect Wallet"]
        TG->>User: shows button
    else No HTTPS URL
        Cmd->>TG: "Wallet connect not configured"
        TG->>User: error message
    end

    User->>MA: taps "Connect Wallet" button
    Note over MA: Telegram opens WebApp

    MA->>MA: Telegram.WebApp.ready()
    MA->>MA: Telegram.WebApp.expand()
    MA->>MA: Extract user from initDataUnsafe

    User->>MA: Connects wallet via RainbowKit<br/>(MetaMask / WalletConnect / Coinbase)

    MA->>MA: useEffect detects isConnected + address
    MA->>MA: bindWallet(address)

    MA->>API: POST /api/wallet/bind
    Note over MA,API: {wallet_address, platform: "telegram",<br/>platform_user_id, init_data}

    API->>API: Validate wallet format (0x + 40 hex)

    alt init_data provided
        API->>API: verifyTelegramAuth(initData, botToken)
        Note over API: HMAC-SHA256 verification
    end

    API->>API: Build session_key = "telegram:dm:{user_id}"

    API->>DB: INSERT INTO users (public_key)<br/>ON CONFLICT DO NOTHING
    API->>DB: INSERT INTO sessions (id, public_key)<br/>ON CONFLICT DO UPDATE SET public_key

    API->>TG: POST /sendMessage<br/>"Wallet connected: 0xABC..."
    TG->>User: confirmation in chat

    API-->>MA: {success: true, wallet_address, session_key}

    MA->>MA: HapticFeedback.success()
    MA->>MA: Show success UI
    MA->>MA: setTimeout → WebApp.close()
```

## 4. Transaction Signing Flow

```mermaid
sequenceDiagram
    actor User
    participant TG as Telegram API
    participant Hdl as handlers.rs
    participant SS as SessionState
    participant Core as CoreApp (LLM)
    participant TxAPI as /api/wallet/tx
    participant MA as Mini App<br/>(/sign page)
    participant Wallet as User Wallet<br/>(MetaMask)

    User->>TG: "send 0.1 ETH to vitalik.eth"
    Note over Hdl: Normal message flow starts...
    Hdl->>SS: send_user_input(text)

    SS--)Core: process_message()
    Core--)Core: LLM decides to call send_tx tool
    Core--)SS: SystemEvent::InlineCall<br/>{type: "wallet_tx_request",<br/>payload: {to, value, data, chainId}}

    Note over Hdl: During poll loop...
    Hdl->>SS: format_session_response()
    SS-->>Hdl: response with system_events

    Hdl->>Hdl: Detect wallet_tx_request event

    Note over Hdl,TxAPI: Create pending transaction
    Hdl->>Hdl: drop(session lock)
    Hdl->>TxAPI: POST /api/wallet/tx<br/>{session_key, tx: {to, value, data, chainId}}
    TxAPI->>TxAPI: Generate txId = "tx_{timestamp}_{random}"
    TxAPI->>TxAPI: Store in pendingTxs Map
    TxAPI-->>Hdl: {txId: "tx_123_abc"}

    Hdl->>Hdl: make_sign_keyboard(tx_id)
    Note over Hdl: WebApp URL: MINI_APP_URL/sign?tx_id=tx_123_abc

    Hdl->>TG: send_message with:<br/>- To: 0xd8dA...<br/>- Amount: 0.100000 ETH<br/>- [Sign Transaction] button
    TG->>User: shows tx details + sign button

    Hdl->>Hdl: re-acquire session lock
    Note over Hdl: Continue poll loop...

    User->>MA: taps "Sign Transaction"
    Note over MA: Opens /sign?tx_id=tx_123_abc

    MA->>TxAPI: GET /api/wallet/tx?tx_id=tx_123_abc
    TxAPI-->>MA: {pending: true, tx: {to, value, chainId}}

    MA->>Wallet: Request signature / sendTransaction
    Wallet->>User: Review transaction popup
    User->>Wallet: Confirm

    Wallet-->>MA: tx_hash

    MA->>TxAPI: PUT /api/wallet/tx<br/>{tx_id, status: "signed", tx_hash}
    TxAPI-->>MA: {success: true}

    MA->>MA: Show success, close WebApp
```

## 5. Command Routing

```mermaid
sequenceDiagram
    actor User
    participant TG as Telegram API
    participant Bot as bot.rs
    participant Cmd as commands.rs
    participant Hdl as handlers.rs

    User->>TG: sends message
    TG->>Bot: Update::filter_message endpoint

    Bot->>Cmd: handle_command(bot, msg, pool, session_mgr)
    Cmd->>Cmd: parse_command(text)

    alt Starts with /
        Cmd->>Cmd: Split command + args
        Cmd->>Cmd: Strip @botname suffix

        alt /start
            Cmd->>Cmd: handle_start()
            Cmd-->>Bot: Ok(true)
        else /connect
            Cmd->>Cmd: handle_connect()
            Cmd-->>Bot: Ok(true)
        else /wallet
            Cmd->>Cmd: handle_wallet()
            Cmd-->>Bot: Ok(true)
        else /disconnect
            Cmd->>Cmd: handle_disconnect()
            Cmd-->>Bot: Ok(true)
        else /sign <tx_id>
            Cmd->>Cmd: handle_sign()
            Cmd-->>Bot: Ok(true)
        else /namespace [args]
            Cmd->>Cmd: handle_namespace()
            Cmd-->>Bot: Ok(true)
        else /model [args]
            Cmd->>Cmd: handle_model()
            Cmd-->>Bot: Ok(true)
        else /help
            Cmd->>Cmd: handle_help()
            Cmd-->>Bot: Ok(true)
        else Unknown /command
            Cmd-->>Bot: Ok(false)
        end
    else Not a command
        Cmd-->>Bot: Ok(false)
    end

    alt Ok(true) — command handled
        Bot-->>TG: respond(())
    else Ok(false) — not a command
        Bot->>Hdl: handle_message()
        Note over Hdl: DM/Group routing
    end
```

## 6. Namespace Switch

```mermaid
sequenceDiagram
    actor User
    participant TG as Telegram API
    participant Cmd as commands.rs
    participant WS as DbWalletConnectService
    participant SM as SessionManager

    User->>TG: /namespace polymarket
    TG->>Cmd: handle_namespace(args="polymarket")

    Cmd->>Cmd: dm_session_key(user_id)
    Cmd->>WS: get_bound_wallet(session_key)
    WS-->>Cmd: Some("0xABC...")

    Cmd->>SM: get_user_namespaces("0xABC...")
    SM-->>Cmd: ["default", "polymarket", "forge"]

    Cmd->>Cmd: NamespaceAuth::new(pub_key, None, "polymarket")
    Cmd->>Cmd: auth.merge_authorization(user_namespaces)
    Cmd->>Cmd: auth.is_authorized() → true

    Cmd->>SM: get_session_config(session_key)
    SM-->>Cmd: (Default, current_selection)

    Cmd->>SM: get_or_create_session(key, auth, selection)
    Note over SM: Replaces backend for session<br/>Cancels old session, creates new one<br/>with Polymarket namespace backend

    SM-->>Cmd: new SessionState

    Cmd->>TG: "Namespace set to polymarket"
    TG->>User: confirmation
```
