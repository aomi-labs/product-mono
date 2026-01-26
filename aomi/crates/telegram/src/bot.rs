use anyhow::Result;
use teloxide::prelude::Bot;

use crate::config::TelegramConfig;

pub struct TelegramBot {
    pub bot: Bot,
    pub config: TelegramConfig,
}

impl TelegramBot {
    pub fn new(config: TelegramConfig) -> Result<Self> {
        let bot = Bot::new(config.bot_token.clone());
        Ok(Self { bot, config })
    }
}
