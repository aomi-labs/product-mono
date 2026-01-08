mod printer;
mod session;
#[cfg(test)]
mod test_app;
mod test_backend;

use std::{
    collections::HashMap,
    io::{self, Write},
    sync::Arc,
    time::Duration,
};

use anyhow::{Context, Result};
use aomi_backend::{BackendType, session::BackendwithTool};
use aomi_chat::{CoreApp, SystemEvent};
use aomi_forge::ForgeApp;
use aomi_l2beat::L2BeatApp;
use clap::{Parser, ValueEnum};
use colored::Colorize;
use printer::{MessagePrinter, render_system_events, split_system_events};
use serde_json::json;
use session::CliSession;
use test_backend::TestBackend;
use tokio::{io::AsyncBufReadExt, sync::mpsc, time};
use tracing_subscriber::EnvFilter;

#[derive(Parser)]
#[command(name = "aomi-cli")]
#[command(about = "Interact with the Aomi agent from your terminal shell.")]
struct Cli {
    /// Send a single prompt and exit (skips interactive mode)
    #[arg(short, long, value_name = "PROMPT")]
    prompt: Option<String>,

    /// Select the backend to target
    #[arg(long, value_enum, default_value_t = BackendSelection::Default)]
    backend: BackendSelection,

    /// Skip loading documentation at startup
    #[arg(long)]
    no_docs: bool,

    /// Skip MCP connections (useful for offline testing)
    #[arg(long)]
    skip_mcp: bool,

    /// Write structured debug logs to the provided path
    #[arg(long)]
    debug_file: Option<String>,

    /// Override the log filter (defaults to aomi_cli=debug)
    #[arg(long, default_value = "debug")]
    log_level: String,

    /// Show tool output content (default prints topic only)
    #[arg(long)]
    show_tool: bool,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, ValueEnum)]
enum BackendSelection {
    Default,
    #[clap(alias = "l2beat")]
    L2b,
    Forge,
    Test,
}

impl From<BackendSelection> for BackendType {
    fn from(value: BackendSelection) -> Self {
        match value {
            BackendSelection::Default => BackendType::Default,
            BackendSelection::L2b => BackendType::L2b,
            BackendSelection::Forge => BackendType::Forge,
            BackendSelection::Test => BackendType::Test,
        }
    }
}

#[tokio::main]
async fn main() -> Result<()> {
    let cli = Cli::parse();
    init_logging(&cli)?;

    let backends = build_backends(cli.no_docs, cli.skip_mcp).await?;
    let mut cli_session = CliSession::new(Arc::clone(&backends), cli.backend.into()).await?;
    let mut printer = MessagePrinter::new(cli.show_tool);

    // Drain initial backend boot logs so the user sees readiness messages
    drain_until_idle(&mut cli_session, &mut printer).await?;

    if let Some(prompt) = cli.prompt {
        run_prompt_mode(&mut cli_session, &mut printer, prompt).await?;
        return Ok(());
    }

    run_interactive_mode(&mut cli_session, &mut printer).await
}

fn init_logging(cli: &Cli) -> Result<()> {
    if let Some(path) = &cli.debug_file {
        let file = std::fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open(path)
            .with_context(|| format!("failed to open debug log file at {path}"))?;

        let filter = if cli.log_level.contains('=') {
            EnvFilter::try_new(&cli.log_level)?
        } else {
            EnvFilter::try_new(format!("aomi_cli={}", cli.log_level))?
        };

        tracing_subscriber::fmt()
            .with_writer(std::sync::Arc::new(file))
            .with_env_filter(filter)
            .init();
    }

    Ok(())
}

async fn run_prompt_mode(
    cli_session: &mut CliSession,
    printer: &mut MessagePrinter,
    prompt: String,
) -> Result<()> {
    cli_session.send_user_input(prompt.trim()).await?;
    drain_until_idle(cli_session, printer).await?;
    Ok(())
}

async fn run_interactive_mode(
    cli_session: &mut CliSession,
    printer: &mut MessagePrinter,
) -> Result<()> {
    println!("Interactive Aomi CLI ready.");
    println!("Commands: :help, :backend <default|l2b|forge|test>, :exit");
    print_prompt()?;
    let mut prompt_visible = true;
    let (tx, mut rx) = mpsc::unbounded_channel::<String>();

    tokio::spawn(async move {
        let mut reader = tokio::io::BufReader::new(tokio::io::stdin());
        loop {
            let mut buffer = String::new();
            let Ok(bytes) = reader.read_line(&mut buffer).await else {
                break;
            };
            if bytes == 0 {
                break;
            }
            if tx.send(buffer).is_err() {
                break;
            }
        }
    });

    let mut tick = time::interval(Duration::from_millis(80));

    loop {
        tokio::select! {
            _ = tick.tick() => {
                cli_session.sync_state().await;
                // Render system events (inline events and async updates)
                let system_events = cli_session.advance_frontend_events();
                let (inline_events, async_updates) = split_system_events(system_events);
                let has_new_output = printer.has_unrendered(cli_session.messages().len())
                    || cli_session.has_streaming_messages()
                    || !inline_events.is_empty()
                    || !async_updates.is_empty();
                if prompt_visible && has_new_output {
                    println!();
                    prompt_visible = false;
                }
                printer.render(cli_session.messages())?;
                if !inline_events.is_empty() || !async_updates.is_empty() {
                    render_system_events(&inline_events, &async_updates)?;
                }
                if !cli_session.is_processing() && !cli_session.has_streaming_messages() && !prompt_visible {
                    print_prompt()?;
                    prompt_visible = true;
                }
            }
            maybe_line = rx.recv() => {
                match maybe_line {
                    Some(line) => {
                        let line = line.trim();
                        prompt_visible = false;
                        match handle_repl_line(cli_session, printer, line).await? {
                            ReplState::Exit => break,
                            ReplState::ImmediatePrompt => {
                                print_prompt()?;
                                prompt_visible = true;
                            }
                            ReplState::AwaitResponse => {}
                        };
                    }
                    None => break,
                }
            }
        }
    }

    Ok(())
}

enum ReplState {
    Exit,
    ImmediatePrompt,
    AwaitResponse,
}

async fn handle_repl_line(
    cli_session: &mut CliSession,
    printer: &mut MessagePrinter,
    line: &str,
) -> Result<ReplState> {
    let trimmed = line.trim();
    if trimmed.is_empty() {
        return Ok(ReplState::ImmediatePrompt);
    }

    if trimmed == ":exit" || trimmed == ":quit" {
        return Ok(ReplState::Exit);
    }

    if trimmed == ":help" {
        println!("Commands:");
        println!("  :help                  Show this message");
        println!("  :backend <name>        Switch backend (default, l2b, forge)");
        println!("  :test-events           Emit mock SystemEvents locally");
        println!("  :exit                  Quit the CLI");
        return Ok(ReplState::ImmediatePrompt);
    }

    if trimmed == ":test-events" {
        cli_session.push_system_event(SystemEvent::InlineDisplay(json!({
            "type": "test_inline",
            "message": "InlineDisplay mock payload",
        })));
        cli_session.push_system_event(SystemEvent::SystemNotice(
            "SystemNotice mock message".to_string(),
        ));
        cli_session.push_system_event(SystemEvent::SystemError(
            "SystemError mock message".to_string(),
        ));
        cli_session.push_system_event(SystemEvent::AsyncUpdate(json!({
            "type": "test_async",
            "message": "AsyncUpdate mock payload",
        })));

        cli_session.sync_state().await;
        printer.render(cli_session.messages())?;
        let system_events = cli_session.advance_frontend_events();
        let (inline_events, async_updates) = split_system_events(system_events);
        if !inline_events.is_empty() || !async_updates.is_empty() {
            render_system_events(&inline_events, &async_updates)?;
        }
        return Ok(ReplState::ImmediatePrompt);
    }

    if let Some(rest) = trimmed.strip_prefix(":backend") {
        let backend_name = rest.trim();
        if backend_name.is_empty() {
            println!("Usage: :backend <default|l2b|forge|Test>");
            return Ok(ReplState::ImmediatePrompt);
        }

        match BackendSelection::from_str(backend_name, true) {
            Ok(target) => {
                cli_session.switch_backend(target.into()).await?;
                cli_session.sync_state().await;
                printer.render(cli_session.messages())?;
            }
            Err(_) => {
                println!("Unknown backend '{backend_name}'. Options: default, l2b, forge");
            }
        }
        return Ok(ReplState::ImmediatePrompt);
    }

    cli_session.send_user_input(trimmed).await?;
    cli_session.sync_state().await;
    printer.render(cli_session.messages())?;
    // Render system events
    let system_events = cli_session.advance_frontend_events();
    let (inline_events, async_updates) = split_system_events(system_events);
    if !inline_events.is_empty() || !async_updates.is_empty() {
        render_system_events(&inline_events, &async_updates)?;
    }
    Ok(ReplState::AwaitResponse)
}

async fn drain_until_idle(session: &mut CliSession, printer: &mut MessagePrinter) -> Result<()> {
    let mut quiet_ticks = 0usize;
    loop {
        session.sync_state().await;
        printer.render(session.messages())?;
        // Render system events
        let system_events = session.advance_frontend_events();
        let (inline_events, async_updates) = split_system_events(system_events);
        if !inline_events.is_empty() || !async_updates.is_empty() {
            render_system_events(&inline_events, &async_updates)?;
        }

        if !session.is_processing() && !session.has_streaming_messages() {
            quiet_ticks += 1;
        } else {
            quiet_ticks = 0;
        }

        if quiet_ticks >= 2 {
            break;
        }

        time::sleep(Duration::from_millis(80)).await;
    }

    Ok(())
}

fn print_prompt() -> io::Result<()> {
    let mut stdout = io::stdout();
    write!(stdout, "{}", "> ".blue().bold())?;
    stdout.flush()
}

async fn build_backends(
    no_docs: bool,
    skip_mcp: bool,
) -> Result<Arc<HashMap<BackendType, Arc<BackendwithTool>>>> {
    let chat_app = Arc::new(
        CoreApp::new_with_options(no_docs, skip_mcp)
            .await
            .map_err(|e| anyhow::anyhow!(e.to_string()))?,
    );
    let l2b_app = Arc::new(
        L2BeatApp::new_with_options(no_docs, skip_mcp)
            .await
            .map_err(|e| anyhow::anyhow!(e.to_string()))?,
    );
    let forge_app = Arc::new(
        ForgeApp::new_with_options(no_docs, skip_mcp)
            .await
            .map_err(|e| anyhow::anyhow!(e.to_string()))?,
    );
    // CLI is used for testing;
    let test_backend = Arc::new(TestBackend::new().await?);

    let chat_backend: Arc<BackendwithTool> = chat_app;
    let l2b_backend: Arc<BackendwithTool> = l2b_app;
    let forge_backend: Arc<BackendwithTool> = forge_app;

    let mut backends: HashMap<BackendType, Arc<BackendwithTool>> = HashMap::new();
    backends.insert(BackendType::Default, chat_backend);
    backends.insert(BackendType::L2b, l2b_backend);
    backends.insert(BackendType::Forge, forge_backend);
    backends.insert(BackendType::Test, test_backend);

    Ok(Arc::new(backends))
}
