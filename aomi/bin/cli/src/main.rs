mod printer;
mod session;

use std::{collections::HashMap, io::{self, Write}, sync::Arc, time::Duration};

use anyhow::{Context, Result};
use aomi_backend::{BackendType, session::ChatBackend};
use aomi_chat::{ChatApp, ToolResultStream};
use aomi_l2beat::L2BeatApp;
use clap::{Parser, ValueEnum};
use printer::MessagePrinter;
use session::CliSession;
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
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, ValueEnum)]
enum BackendSelection {
    Default,
    #[clap(alias = "l2beat")]
    L2b,
}

impl From<BackendSelection> for BackendType {
    fn from(value: BackendSelection) -> Self {
        match value {
            BackendSelection::Default => BackendType::Default,
            BackendSelection::L2b => BackendType::L2b,
        }
    }
}

#[tokio::main]
async fn main() -> Result<()> {
    let cli = Cli::parse();
    init_logging(&cli)?;

    let backends = build_backends(cli.no_docs, cli.skip_mcp).await?;
    let mut session = CliSession::new(Arc::clone(&backends), cli.backend.into()).await?;
    let mut printer = MessagePrinter::new();

    // Drain initial backend boot logs so the user sees readiness messages
    drain_until_idle(&mut session, &mut printer).await?;

    if let Some(prompt) = cli.prompt {
        run_prompt_mode(&mut session, &mut printer, prompt).await?;
        return Ok(());
    }

    run_interactive_mode(&mut session, &mut printer).await
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
    session: &mut CliSession,
    printer: &mut MessagePrinter,
    prompt: String,
) -> Result<()> {
    session.send_user_message(prompt.trim()).await?;
    drain_until_idle(session, printer).await?;
    Ok(())
}

async fn run_interactive_mode(session: &mut CliSession, printer: &mut MessagePrinter) -> Result<()> {
    println!("Interactive Aomi CLI ready.");
    println!("Commands: :help, :backend <default|l2b>, :exit");
    print_prompt()?;

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
                session.update_state().await;
                printer.render(session.messages())?;
            }
            maybe_line = rx.recv() => {
                match maybe_line {
                    Some(line) => {
                        if handle_repl_line(session, printer, line.trim()).await? {
                            break;
                        }
                    }
                    None => break,
                }
                print_prompt()?;
            }
        }
    }

    Ok(())
}

async fn handle_repl_line(
    session: &mut CliSession,
    printer: &mut MessagePrinter,
    line: &str,
) -> Result<bool> {
    let trimmed = line.trim();
    if trimmed.is_empty() {
        return Ok(false);
    }

    if trimmed == ":exit" || trimmed == ":quit" {
        return Ok(true);
    }

    if trimmed == ":help" {
        println!("Commands:");
        println!("  :help                  Show this message");
        println!("  :backend <name>        Switch backend (default, l2b)");
        println!("  :exit                  Quit the CLI");
        return Ok(false);
    }

    if let Some(rest) = trimmed.strip_prefix(":backend") {
        let backend_name = rest.trim();
        if backend_name.is_empty() {
            println!("Usage: :backend <default|l2b>");
            return Ok(false);
        }

        match BackendSelection::from_str(backend_name, true) {
            Ok(target) => {
                session.switch_backend(target.into()).await?;
                session.update_state().await;
                printer.render(session.messages())?;
            }
            Err(_) => {
                println!("Unknown backend '{backend_name}'. Options: default, l2b");
            }
        }
        return Ok(false);
    }

    session.send_user_message(trimmed).await?;
    session.update_state().await;
    printer.render(session.messages())?;
    Ok(false)
}

async fn drain_until_idle(session: &mut CliSession, printer: &mut MessagePrinter) -> Result<()> {
    let mut quiet_ticks = 0usize;
    loop {
        session.update_state().await;
        printer.render(session.messages())?;

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
    write!(stdout, "> ")?;
    stdout.flush()
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
