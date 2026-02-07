# Telegram Bot — Call Stacks

## 1. Message Processing (DM)

```
teloxide::Dispatcher::dispatch()                         # long-polling loop
└─ bot.rs endpoint closure(msg, bot_ref, session_mgr)
   ├─ commands::handle_command()                         # returns Ok(false) — not a command
   └─ handlers::handle_message(bot, msg, session_mgr)
      └─ handle_dm(bot, msg, session_mgr)                # chat.is_private() = true
         ├─ session::user_id_from_message(msg)
         ├─ config.dm_policy check                       # Open / Allowlist / Disabled
         ├─ session::dm_session_key(user_id)             # → "telegram:dm:123"
         └─ process_and_respond(bot, msg, sm, key, text)
            ├─ DbWalletConnectService::get_bound_wallet(key)
            ├─ SessionManager::get_session_config(key)   # → (Namespace, Selection)
            ├─ NamespaceAuth::new(wallet, None, namespace)
            ├─ SessionManager::get_or_create_session(key, auth, None)
            ├─ Bot.send_chat_action(Typing)
            ├─ SessionState::sync_user_state(UserState)  # inject wallet
            ├─ SessionState::send_user_input(text)       # kicks off background LLM task
            ├─ [poll loop — 100ms tick, 60s max]
            │  ├─ SessionState::sync_state()
            │  ├─ SessionState::format_session_response()
            │  ├─ check system_events for wallet_tx_request
            │  │  └─ handle_wallet_tx_request() if found
            │  ├─ break if is_processing = false
            │  └─ Bot.send_chat_action(Typing) every 4s
            ├─ extract_assistant_text(response)
            ├─ send::format_for_telegram(text)
            │  ├─ markdown_to_telegram_html()
            │  └─ chunk_message(html, 4000)
            └─ Bot.send_message(chunk, HTML) for each chunk
```

## 2. Group Message Processing

```
handlers::handle_message(bot, msg, session_mgr)
└─ handle_group(bot, msg, session_mgr)                   # chat.is_group() = true
   ├─ session::user_id_from_message(msg)
   ├─ config.group_policy check                          # Mention / Always / Disabled
   ├─ is_bot_mentioned(bot, msg)                         # if policy = Mention
   │  ├─ bot.get_me()                                    # get bot username
   │  ├─ check reply_to_message.from == bot              # reply to bot?
   │  └─ check message entities for @bot_username        # mentioned?
   ├─ session::group_session_key(chat_id)                # → "telegram:group:-100123"
   └─ process_and_respond(...)                           # same as DM from here
```

## 3. Command Dispatch

```
bot.rs endpoint closure
└─ commands::handle_command(bot, msg, pool, session_mgr)
   └─ parse_command(text)                                # split "/" + args, strip @botname
      ├─ "start"      → handle_start()                  # welcome msg + connect keyboard
      ├─ "connect"    → handle_connect()                 # WebApp connect keyboard
      ├─ "wallet"     → handle_wallet()                  # show bound wallet
      ├─ "disconnect" → handle_disconnect()              # unbind wallet
      ├─ "sign"       → handle_sign(tx_id)               # sign keyboard for tx
      ├─ "help"       → handle_help()                    # list commands
      ├─ "namespace"  → handle_namespace(args)           # see below
      ├─ "model"      → handle_model(args)               # see below
      └─ unknown      → Ok(false)                        # fall through to handle_message
```

## 4. Namespace Switch

```
commands::handle_namespace(bot, msg, pool, sm, args)
├─ session::dm_session_key(user_id)
├─ DbWalletConnectService::get_bound_wallet(key)
├─ SessionManager::get_user_namespaces(public_key)
├─ NamespaceAuth::new(pub_key, None, None)
├─ auth.merge_authorization(user_namespaces)
└─ match args:
   ├─ "" or "show"  → display current namespace
   ├─ "list"        → display all authorized namespaces
   └─ <name>
      ├─ Namespace::parse(arg)
      ├─ NamespaceAuth::new(pub_key, None, name)
      ├─ auth.is_authorized()                            # check wallet has access
      └─ SessionManager::get_or_create_session(key, auth, selection)
         # replaces backend, cancels old session
```

## 5. Model Switch

```
commands::handle_model(bot, msg, pool, sm, args)
├─ session::dm_session_key(user_id)
├─ DbWalletConnectService::get_bound_wallet(key)
├─ SessionManager::get_session_config(key)               # → (Namespace, Selection)
└─ match args:
   ├─ "" or "show"  → display current model
   ├─ "list"        → AomiModel::rig_all()
   └─ <slug>
      ├─ AomiModel::parse_rig(arg)
      ├─ selection.rig = model
      └─ SessionManager::get_or_create_session(key, auth, selection)
```

## 6. Wallet Connect

```
commands::handle_connect(bot, msg)
├─ get_mini_app_url()                                    # env MINI_APP_URL, must be HTTPS
├─ make_connect_keyboard()                               # InlineKeyboardButton::web_app
└─ Bot.send_message(keyboard)                            # user sees "Connect Wallet" button

[user taps button → Telegram opens WebApp]

mini-app connect-wallet.tsx
├─ Telegram.WebApp.ready()
├─ Telegram.WebApp.expand()
├─ extract user from initDataUnsafe
├─ user connects wallet via RainbowKit
├─ useEffect detects isConnected + address
└─ bindWallet(address)
   └─ POST /api/wallet/bind
      ├─ validate wallet format (0x + 40 hex)
      ├─ verifyTelegramAuth(initData, botToken)          # HMAC-SHA256
      ├─ build session_key = "telegram:dm:{user_id}"
      ├─ INSERT INTO users (public_key) ON CONFLICT DO NOTHING
      ├─ INSERT INTO sessions (id, public_key) ON CONFLICT UPDATE
      ├─ POST telegram.org/sendMessage "Wallet connected"
      └─ return {success, wallet_address, session_key}

[mini-app shows success → HapticFeedback → WebApp.close()]
```

## 7. Transaction Signing

```
[during poll loop in process_and_respond]

handlers::process_and_respond — poll loop
├─ format_session_response() returns SystemEvent::InlineCall
│  type = "wallet_tx_request", payload = {to, value, data, chainId}
└─ handle_wallet_tx_request(bot, msg, session_key, payload)
   └─ create_pending_tx(session_key, tx)
      └─ POST {MINI_APP_URL}/api/wallet/tx
         ├─ generate txId = "tx_{timestamp}_{random}"
         ├─ pendingTxs.set(txId, {sessionKey, tx, status: "pending"})
         └─ return {txId}
   ├─ format tx details (to, value wei→ETH, description)
   ├─ make_sign_keyboard(tx_id)                          # WebApp: /sign?tx_id=...
   └─ Bot.send_message(tx_details + sign_button)

[user taps "Sign Transaction" → mini-app opens]

mini-app /sign?tx_id=...
├─ GET /api/wallet/tx?tx_id=...                          # fetch tx details
├─ display tx for user review
├─ user signs with wallet (MetaMask)
└─ PUT /api/wallet/tx {tx_id, status: "signed", tx_hash}
```

## 8. Formatting Pipeline

```
send::format_for_telegram(markdown)
├─ markdown_to_telegram_html(markdown)
│  ├─ escape_html()                                      # & < > "
│  ├─ regex: `code` → <code>
│  ├─ regex: [text](url) → <a href>
│  ├─ regex: **bold** → <b>
│  └─ regex: *italic* → <i>
└─ chunk_message(html, 4000)
   ├─ if len ≤ 4000 → single chunk
   └─ else split at newlines
      └─ if single line > 4000 → hard split by char count
→ Vec<String> chunks (empty chunks filtered out)
```
