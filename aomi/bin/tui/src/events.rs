use anyhow::Result;
use crossterm::event::{self, Event as CrosstermEvent, KeyEvent, MouseEvent};
use std::time::Duration;
use tokio::time::interval;

#[derive(Debug)]
pub enum Event {
    Key(KeyEvent),
    Mouse(MouseEvent),
    Tick,
}

pub struct EventHandler {
    tick_interval: tokio::time::Interval,
}

impl EventHandler {
    pub fn new() -> Self {
        Self {
            tick_interval: interval(Duration::from_millis(250)),
        }
    }

    pub async fn next(&mut self) -> Result<Event> {
        tokio::select! {
            _ = self.tick_interval.tick() => {
                Ok(Event::Tick)
            }
            _ = tokio::task::spawn_blocking(|| {
                event::poll(Duration::from_millis(50))
            }) => {
                if event::poll(Duration::ZERO)? {
                    match event::read()? {
                        CrosstermEvent::Key(key) => Ok(Event::Key(key)),
                        CrosstermEvent::Mouse(mouse) => Ok(Event::Mouse(mouse)),
                        _ => Ok(Event::Tick),
                    }
                } else {
                    Ok(Event::Tick)
                }
            }
        }
    }
}
