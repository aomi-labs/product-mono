mod app;
mod events;
mod messages;
mod ui;

use anyhow::Result;
use clap::Parser;
use crossterm::{
    event::{DisableMouseCapture, EnableMouseCapture},
    execute,
    terminal::{EnterAlternateScreen, LeaveAlternateScreen, disable_raw_mode, enable_raw_mode},
};
use ratatui::{Terminal, backend::CrosstermBackend};
use std::io;

use crate::app::SessionContainer;
use crate::events::EventHandler;

#[derive(Parser)]
#[command(name = "aomi")]
#[command(about = "Agentic EVM oPURRator")]
struct Cli {
    /// Skip loading Uniswap documentation at startup
    #[arg(long)]
    no_docs: bool,

    /// Skip MCP server connection (for testing)
    #[arg(long)]
    skip_mcp: bool,
}

#[tokio::main]
async fn main() -> Result<()> {
    let cli = Cli::parse();

    // Explicitly set RUST_LOG to empty to disable all logging
    unsafe {
        std::env::set_var("RUST_LOG", "");
    }

    // Create app BEFORE setting up terminal so we can see any panics
    let app = match SessionContainer::new(cli.no_docs, cli.skip_mcp).await {
        Ok(app) => app,
        Err(e) => {
            eprintln!("Failed to initialize app: {e:?}");
            eprintln!("Press Enter to exit...");
            let _ = std::io::stdin().read_line(&mut String::new());
            return Err(e);
        }
    };

    // Setup terminal only after app is successfully created
    enable_raw_mode()?;
    let mut stdout = io::stdout();
    execute!(stdout, EnterAlternateScreen, EnableMouseCapture)?;
    let backend = CrosstermBackend::new(stdout);
    let mut terminal = Terminal::new(backend)?;
    let res = run_app(&mut terminal, app).await;

    // Restore terminal
    disable_raw_mode()?;
    execute!(
        terminal.backend_mut(),
        LeaveAlternateScreen,
        DisableMouseCapture
    )?;
    terminal.show_cursor()?;

    if let Err(err) = res {
        eprintln!("{err:?}");
    }

    Ok(())
}

async fn run_app<B: ratatui::backend::Backend>(
    terminal: &mut Terminal<B>,
    mut app: SessionContainer,
) -> Result<()> {
    let mut event_handler = EventHandler::new();

    loop {
        terminal.draw(|f| ui::draw(f, &mut app))?;

        match event_handler.next().await? {
            events::Event::Key(key_event) => {
                if app.handle_key_event(key_event).await? {
                    return Ok(());
                }
            }
            events::Event::Mouse(mouse_event) => {
                if app.handle_mouse_event(mouse_event).await? {
                    return Ok(());
                }
            }
            events::Event::Tick => {
                app.on_tick().await;
            }
        }
    }
}
