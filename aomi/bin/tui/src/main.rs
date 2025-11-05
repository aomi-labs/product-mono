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
use std::{collections::HashMap, io, sync::Arc};
use tracing_subscriber::EnvFilter;
use aomi_chat::{ChatApp, ToolResultStream};
use aomi_l2beat::L2BeatApp;
use aomi_backend::{BackendType, session::ChatBackend};

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

    let backends = match build_backends(cli.no_docs, cli.skip_mcp).await {
        Ok(backends) => backends,
        Err(e) => {
            eprintln!("Failed to initialize backends: {e:?}");
            eprintln!("Press Enter to exit...");
            let _ = std::io::stdin().read_line(&mut String::new());
            return Err(e);
        }
    };

    // Create app BEFORE setting up terminal so we can see any panics
    let app = match SessionContainer::new(backends).await {
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

async fn build_backends(
    no_docs: bool,
    skip_mcp: bool,
) -> Result<Arc<HashMap<BackendType, Arc<dyn ChatBackend<ToolResultStream>>>>> {
    let chat_app = Arc::new(
        ChatApp::new_with_options(no_docs, skip_mcp)
            .await
            .map_err(|e| anyhow::anyhow!(e.to_string()))?,
    );
    let l2b_app = Arc::new(
        L2BeatApp::new_with_options(no_docs, skip_mcp)
            .await
            .map_err(|e| anyhow::anyhow!(e.to_string()))?,
    );

    let chat_backend: Arc<dyn ChatBackend<ToolResultStream>> = chat_app;
    let l2b_backend: Arc<dyn ChatBackend<ToolResultStream>> = l2b_app;

    let mut backends: HashMap<BackendType, Arc<dyn ChatBackend<ToolResultStream>>> = HashMap::new();
    backends.insert(BackendType::Default, chat_backend);
    backends.insert(BackendType::L2b, l2b_backend);

    Ok(Arc::new(backends))
}
