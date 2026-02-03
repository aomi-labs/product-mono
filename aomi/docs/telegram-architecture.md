# Telegram Integration Architecture

## Overview
Add Telegram Bot API support to Aomi, following Clawdbot's integration pattern adapted for Rust.

## Dependencies
- `teloxide` - Rust Telegram Bot framework (similar to grammY in TypeScript)
- `tokio` - Async runtime (already used)
- Integrate with existing `aomi-backend` session management

## Crate Structure
New crate: `aomi/crates/telegram/`
```
telegram/
├── Cargo.toml
├── src/
│   ├── lib.rs           # Public API
│   ├── bot.rs           # Bot creation and configuration
│   ├── handlers.rs      # Message handlers
│   ├── context.rs       # Message context (sender, chat, etc.)
│   ├── commands.rs      # Native commands (/status, /reset, etc.)
│   ├── session.rs       # Session key routing
│   ├── send.rs          # Outbound message formatting
│   └── config.rs        # Telegram-specific configuration
```

## Key Components

### 1. TelegramBot (bot.rs)
- Create bot with token
- Configure long-polling or webhook
- Set up update handlers
- Throttling and sequentialization

### 2. Message Handlers (handlers.rs)
- Route DMs to main session
- Route groups to group-specific sessions
- Handle mentions in groups
- Process callbacks/inline buttons

### 3. Session Integration (session.rs)
- Map Telegram chat_id → SessionState
- Use existing SessionManager from aomi-backend
- Session key format: `telegram:dm:{user_id}` or `telegram:group:{chat_id}`

### 4. Commands (commands.rs)
- /status - Show session status
- /reset - Reset session
- /help - Show available commands

### 5. Message Formatting (send.rs)
- Convert markdown to Telegram HTML
- Chunk long messages (4000 char limit)
- Handle media attachments

## Integration Points
1. Add `aomi-telegram` to workspace Cargo.toml
2. Backend binary starts Telegram bot alongside HTTP server
3. Share SessionManager between HTTP and Telegram channels

## Configuration
```yaml
telegram:
  enabled: true
  bot_token: "env:TELEGRAM_BOT_TOKEN"
  dm_policy: "open"  # open | allowlist | disabled
  group_policy: "mention"  # mention | always | disabled
  allow_from: []  # User IDs for allowlist mode
```

## Flow
1. User sends message → teloxide receives update
2. Handler extracts context (user, chat, text)
3. Resolve session key from chat type
4. Get or create SessionState via SessionManager
5. Process message through existing backend pipeline
6. Send response back via Telegram API
