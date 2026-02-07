mod app;
mod events;
mod messages;
mod ui;

use anyhow::Result;
use aomi_backend::{BuildOpts, Namespace, build_backends};
use aomi_core::{AomiModel, Selection};
use clap::{ArgAction, Parser};
use crossterm::{
    event::{DisableMouseCapture, EnableMouseCapture},
    execute,
    terminal::{EnterAlternateScreen, LeaveAlternateScreen, disable_raw_mode, enable_raw_mode},
};
use ratatui::{Terminal, backend::CrosstermBackend};
use std::{io, sync::Arc};
use tracing_subscriber::EnvFilter;

use crate::app::SessionContainer;
use crate::events::EventHandler;

#[derive(Parser, Clone)]
#[command(name = "aomi")]
#[command(about = "Agentic EVM oPURRator")]
pub struct Cli {
    /// Skip loading Uniswap documentation at startup
    #[arg(long, default_value_t = true, action = ArgAction::Set)]
    no_docs: bool,

    /// Skip MCP server connection (for testing)
    #[arg(long, default_value_t = true, action = ArgAction::Set)]
    skip_mcp: bool,

    /// Enable debug logging to file
    #[arg(long)]
    debug_file: Option<String>,

    /// Set log level filter (e.g., debug, aomi_tui=debug, aomi_tui=debug,other_crate=info)
    #[arg(long, default_value = "debug")]
    log_level: String,
}

#[tokio::main]
async fn main() -> Result<()> {
    let cli = Cli::parse();

    // Setup logging
    if let Some(debug_file) = &cli.debug_file {
        let file = std::fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open(debug_file)?;

        // Build filter: if log_level contains '=', treat as a full filter string
        // Otherwise, apply it only to aomi_tui crate
        let filter = if cli.log_level.contains('=') {
            EnvFilter::try_new(&cli.log_level)?
        } else {
            EnvFilter::try_new(format!("aomi_tui={}", cli.log_level))?
        };

        tracing_subscriber::fmt()
            .with_writer(std::sync::Arc::new(file))
            .with_env_filter(filter)
            .init();

        tracing::debug!("\n\nStarting aomi-tui");
        tracing::debug!("no_docs: {}", cli.no_docs);
        tracing::debug!("skip_mcp: {}", cli.skip_mcp);
        tracing::debug!("debug_file: {:?}", cli.debug_file);
    }

    let selection = Selection {
        rig: AomiModel::ClaudeOpus4,
        baml: AomiModel::ClaudeOpus4,
    };
    let opts = BuildOpts {
        no_docs: cli.no_docs,
        skip_mcp: cli.skip_mcp,
        no_tools: false,
        selection,
    };
    let backends = match build_backends(vec![
        (Namespace::Default, opts),
        (Namespace::L2b, opts),
        (Namespace::Forge, opts),
    ])
    .await
    {
        Ok(backends) => Arc::new(backends),
        Err(e) => {
            eprintln!("Failed to initialize backends: {e:?}");
            eprintln!("Press Enter to exit...");
            let _ = std::io::stdin().read_line(&mut String::new());
            return Err(e);
        }
    };

    // Create app BEFORE setting up terminal so we can see any panics
    let app = match SessionContainer::new(backends, opts).await {
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
