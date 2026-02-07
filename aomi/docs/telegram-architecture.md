# Telegram Bot Architecture

## Overview

Telegram bot for Aomi, built on `teloxide` with a panel state machine for UI dispatch.

## Crate Structure

```
aomi/bin/telegram/src/
├── main.rs           # Entry point, CLI args, DB + SessionManager init
├── bot.rs            # TelegramBot struct, dptree dispatch, long-polling
├── config.rs         # TelegramConfig (from bot.toml), DmPolicy, GroupPolicy
├── handlers.rs       # DM/group routing, process_and_respond loop, tx signing
├── commands.rs       # Non-panel commands: parse_command, handle_help, handle_sign
├── send.rs           # escape_html, markdown_to_telegram_html, chunk_message
├── session.rs        # Session key builders (dm/group/topic)
└── panels/
    ├── mod.rs        # Panel trait, PanelCtx, PanelView, Transition, PanelRouter
    ├── start.rs      # Welcome grid (8-button navigation)
    ├── namespace.rs  # Namespace picker + set
    ├── model.rs      # Model picker + set
    ├── sessions.rs   # Session list + select
    ├── status.rs     # Backend state display
    ├── wallet.rs     # Connect / create / disconnect wallet
    ├── apikey.rs     # ForceReply prompt + key validation
    └── settings.rs   # Archive session / delete wallet
```

## Panel State Machine

### Core Types (panels/mod.rs)

```
PanelCtx          Shared request context (bot, pool, session_manager, chat_id, ...)
PanelView         Render output: { text, keyboard }
Transition        Handler return: Navigate | Render | Toast | ToastHtml | None
Panel trait       prefix(), commands(), render(), on_callback(), on_command(), on_text()
PanelRouter       Dispatches callbacks/commands to panels by prefix
```

### Callback Data Format

All panel button callbacks use the format `p:{prefix}:{action}`:

```
p:start                    -> StartPanel.render()
p:ns                       -> NamespacePanel.render()
p:ns:set:polymarket        -> NamespacePanel.on_callback("set:polymarket")
p:model:set:opus-4         -> ModelPanel.on_callback("set:opus-4")
p:sess:sel:abc123          -> SessionsPanel.on_callback("sel:abc123")
p:settings:archive         -> SettingsPanel.on_callback("archive")
p:wallet:create            -> WalletPanel.on_callback("create")
p:apikey                   -> ApiKeyPanel.render() (sends ForceReply)
```

Legacy callback data (`panel:namespace`, `namespace_set:X`, etc.) is normalized
to the `p:` format via `normalize_callback_data()`.

### Dispatch Flow

```
Update arrives (teloxide long-polling)
  │
  ├── CallbackQuery
  │     ├── answer_callback_query()
  │     ├── normalize_callback_data() (legacy compat)
  │     └── PanelRouter.handle_callback(ctx, data)
  │           ├── strip "p:" prefix
  │           ├── split prefix:action
  │           ├── empty action → panel.render()
  │           └── action → panel.on_callback(action)
  │
  └── Message
        ├── parse_command(text)
        │     ├── PanelRouter.handle_command(ctx, cmd, args)
        │     │     ├── on_command() returns Transition
        │     │     └── Transition::None → fallback to render()
        │     ├── handle_sign() / handle_help() (non-panel)
        │     └── unknown → fall through
        │
        ├── is_api_key_reply?
        │     └── PanelRouter.handle_text("apikey", ctx, text)
        │
        └── process_and_respond()
              ├── auth + get_or_create_session
              ├── send_user_input → poll loop
              ├── handle wallet_tx_request events
              └── format + send response chunks
```

## Panels

| Panel | Prefix | Commands | Key behavior |
|-------|--------|----------|-------------|
| Start | `start` | `/start` | 7-button navigation grid |
| Namespace | `ns` | `/namespace` | Picker + direct set, DM-only, auth check |
| Model | `model` | `/model` | Picker + direct set, DM-only, auth check |
| Sessions | `sess` | `/sessions` | List sessions by wallet, fallback to connect |
| Status | `status` | `/status` | Fetch backend state, chain info, display |
| Wallet | `wallet` | `/connect`, `/wallet`, `/disconnect` | WebApp buttons or fallback callbacks |
| API Key | `apikey` | `/apikey` | ForceReply prompt, validate via AuthorizedKey |
| Settings | `settings` | `/settings` | Archive session, delete wallet, DM-only |

## Configuration (bot.toml)

```toml
enabled = true
bot_token = "..."
dm_policy = "open"        # open | allowlist | disabled
group_policy = "mention"  # mention | always | disabled
backend_url = "..."       # optional, for /status panel
allow_from = []           # user IDs for allowlist mode
```

## Dependencies

- `teloxide` — Telegram Bot framework
- `dptree` — Handler tree for routing updates
- `aomi-backend` — SessionManager, NamespaceAuth, AuthorizedKey
- `aomi-bot-core` — DbWalletConnectService, extract_assistant_text
- `aomi-anvil` — ProviderManager (chain info for /status)

## Session Keys

```
DM:      telegram:dm:{user_id}
Group:   telegram:group:{chat_id}
Topic:   telegram:group:{chat_id}:thread:{thread_id}
```
