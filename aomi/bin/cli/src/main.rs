mod printer;
mod session;
#[cfg(test)]
mod test_app;
mod test_backend;

use std::{
    io::{self, Write},
    sync::Arc,
    time::Duration,
};

use aomi_backend::{BuildOpts, Namespace, build_backends};
use aomi_core::{AomiModel, Selection, SystemEvent};
use clap::{Parser, ValueEnum};
use colored::Colorize;
use eyre::{Context, Result};
use printer::{MessagePrinter, render_system_events, split_system_events};
use serde_json::json;
use session::CliSession;
use test_backend::TestSchedulerBackend;
use tokio::{io::AsyncBufReadExt, sync::{mpsc, RwLock}, time};
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
    Polymarket,
    Test,
}

impl From<BackendSelection> for Namespace {
    fn from(value: BackendSelection) -> Self {
        match value {
            BackendSelection::Default => Namespace::Default,
            BackendSelection::L2b => Namespace::L2b,
            BackendSelection::Forge => Namespace::Forge,
            BackendSelection::Polymarket => Namespace::Polymarket,
            BackendSelection::Test => Namespace::Test,
        }
    }
}

#[tokio::main]
async fn main() -> Result<()> {
    let cli = Cli::parse();
    init_logging(&cli)?;

    let selection = Selection {
        rig: AomiModel::ClaudeSonnet4,
        baml: AomiModel::ClaudeOpus4,
    };
    let opts = BuildOpts {
        no_docs: cli.no_docs,
        skip_mcp: cli.skip_mcp,
        no_tools: false,
        selection,
    };
    let backends = build_backends(vec![
        (Namespace::Default, opts),
        (Namespace::L2b, opts),
        (Namespace::Forge, opts),
    ])
    .await
    .map_err(|e| eyre::eyre!(e.to_string()))?;
    let mut backends = backends;
    let test_backend = Arc::new(
        TestSchedulerBackend::new()
            .await
            .map_err(|e| eyre::eyre!(e.to_string()))?,
    );
    backends.insert(Namespace::Test, test_backend);
    let backends = Arc::new(RwLock::new(backends));

    let mut cli_session =
        CliSession::new(Arc::clone(&backends), cli.backend.into(), opts).await?;
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
    println!("Commands: :help, :backend <default|l2b|forge|test>, /model, :exit");
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
        println!("  /model main            Use Rig model selection (main)");
        println!("  /model small           Use BAML model selection (small)");
        println!("  /model list            Show available models");
        println!("  /model show            Show current model selection");
        println!("  :test-events           Emit mock SystemEvents locally");
        println!("  :exit                  Quit the CLI");
        return Ok(ReplState::ImmediatePrompt);
    }

    if trimmed == ":test-events" {
        cli_session.push_system_event(SystemEvent::InlineCall(json!({
            "type": "test_inline",
            "message": "InlineCall mock payload",
        })));
        cli_session.push_system_event(SystemEvent::SystemNotice(
            "SystemNotice mock message".to_string(),
        ));
        cli_session.push_system_event(SystemEvent::SystemError(
            "SystemError mock message".to_string(),
        ));
        cli_session.push_system_event(SystemEvent::AsyncCallback(json!({
            "type": "test_async",
            "message": "AsyncCallback mock payload",
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
            println!("Usage: :backend <default|l2b|forge|test>");
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

    if let Some(rest) = trimmed.strip_prefix("/model") {
        let command = rest.trim();
        if command.is_empty() {
            println!("Usage: /model main|small|list|show");
            return Ok(ReplState::ImmediatePrompt);
        }

        let mut parts = command.split_whitespace();
        let action = parts.next().unwrap_or("");
        let arg = parts.next();

        match action {
            "main" => {
                let model = match arg {
                    Some(value) => AomiModel::parse_rig(value)
                        .unwrap_or(AomiModel::ClaudeSonnet4),
                    None => AomiModel::ClaudeSonnet4,
                };
                let baml_model = AomiModel::parse_baml(cli_session.baml_client())
                    .unwrap_or(AomiModel::ClaudeOpus4);
                cli_session.set_models(model, baml_model).await?;
                println!(
                    "Model selection updated: rig={} baml={}",
                    model.rig_slug(),
                    cli_session.baml_client()
                );
            }
            "small" => {
                let model = match arg {
                    Some(value) => AomiModel::parse_baml(value)
                        .unwrap_or(AomiModel::ClaudeOpus4),
                    None => AomiModel::ClaudeOpus4,
                };
                cli_session
                    .set_models(cli_session.rig_model(), model)
                    .await?;
                println!(
                    "Model selection updated: rig={} baml={}",
                    cli_session.rig_model().rig_slug(),
                    model.baml_client_name()
                );
            }
            "list" => {
                println!("Rig models:");
                for model in AomiModel::rig_all() {
                    println!("  {} ({})", model.rig_label(), model.rig_slug());
                }
                println!("BAML clients:");
                for model in AomiModel::baml_all() {
                    println!("  {} ({})", model.baml_label(), model.baml_client_name());
                }
            }
            "show" => {
                println!("Models: {}", cli_session.models_summary().await?);
            }
            _ => {
                println!("Unknown model action '{action}'. Use /model list.");
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

        // Wait until LLM is done AND all tool calls have completed
        let has_ongoing_tools = session.has_ongoing_tool_calls().await;
        if !session.is_processing() && !session.has_streaming_messages() && !has_ongoing_tools {
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
