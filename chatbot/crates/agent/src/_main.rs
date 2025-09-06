use colored::*;
use eyre::Result;
use rig::{message::Message, prelude::*};
use rmcp::{
    ServiceExt,
    model::{ClientCapabilities, ClientInfo, Implementation, Tool as RmcpTool},
    transport::StreamableHttpClientTransport,
};
use rustyline::DefaultEditor;
use std::{env, sync::Arc, time::Duration};

use rig::providers;

use crate::{
    accounts::generate_account_context,
    helpers::{custom_stream_to_stdout, multi_turn_prompt},
};

mod accounts;
mod erc20;
mod helpers;

const CLAUDE_3_5_SONNET: &str = "claude-3-5-sonnet-20241022";

#[tokio::main]
async fn main() -> Result<(), eyre::Error> {
    // tracing_subscriber::fmt()
    //     .with_env_filter(EnvFilter::from_default_env().add_directive(tracing::Level::INFO.into()))
    //     .with_writer(std::io::stderr)
    //     .init();

    let anthropic_client = providers::anthropic::Client::new(
        &env::var("ANTHROPIC_API_KEY").expect("ANTHROPIC_API_KEY not set"),
    );

    let transport = StreamableHttpClientTransport::from_uri("http://127.0.0.1:3000");

    let client_info = ClientInfo {
        protocol_version: Default::default(),
        capabilities: ClientCapabilities::default(),
        client_info: Implementation {
            name: "rig-core".to_string(),
            version: "0.13.0".to_string(),
        },
    };

    let client = client_info.serve(transport).await.inspect_err(|e| {
        tracing::error!("client error: {:?}", e);
    })?;

    let server_info = client.peer_info();
    tracing::info!("Connected to server: {server_info:#?}");

    let tools: Vec<RmcpTool> = client.list_tools(Default::default()).await?.tools;

    let agent = anthropic_client
        .agent(CLAUDE_3_5_SONNET)
        .preamble(format!("You are a helpful assistant for Ethereum operations. When handling requests that require multiple steps:
1. Explain what you're about to do in natural, conversational language
2. Execute the action
3. After seeing the result, explain what you'll do next (if there are more steps)
4. Continue this pattern until the task is complete

Be conversational and dynamic - react to the results you see and adjust your approach accordingly. Don't just log actions, have a natural flow of explanation and execution.

{}", generate_account_context()).as_str())
        .tool(erc20::GenerateErc20TransferCalldata)
        .tool(erc20::GenerateErc20ApproveCalldata)
        .tool(erc20::GenerateErc20TransferFromCalldata)
        .tool(erc20::GenerateErc20BalanceOfCalldata)
        .tool(erc20::GenerateErc20AllowanceCalldata)
        .tool(erc20::GenerateErc20NameCalldata)
        .tool(erc20::GenerateErc20SymbolCalldata)
        .tool(erc20::GenerateErc20DecimalsCalldata)
        .tool(erc20::GenerateErc20TotalSupplyCalldata);
    let agent = tools
        .into_iter()
        .fold(agent, |agent, tool| agent.rmcp_tool(tool, client.clone()))
        .build();
    let agent = Arc::new(agent);

    let mut rl = DefaultEditor::new()?;
    let mut chat_history = Vec::new();

    loop {
        let prompt = "> ".bright_yellow().to_string();
        let line = rl.readline(&prompt)?;

        match line.as_str() {
            "quit" | "exit" => break,
            input => {
                // Display the user's input in a different color
                println!("{}", input.cyan());

                // Show a simple animated indicator
                let spinner_chars = vec!["⠋", "⠙", "⠹", "⠸", "⠼", "⠴", "⠦", "⠧", "⠇", "⠏"];
                let spinner_task = tokio::spawn(async move {
                    let mut i = 0;
                    loop {
                        print!(
                            "\r{} {}",
                            spinner_chars[i].bright_cyan().bold(),
                            "Thinking...".cyan()
                        );
                        std::io::Write::flush(&mut std::io::stdout()).unwrap();
                        i = (i + 1) % spinner_chars.len();
                        tokio::time::sleep(Duration::from_millis(100)).await;
                    }
                });

                // Prompt the agent and get the stream
                let mut stream =
                    multi_turn_prompt(agent.clone(), input, chat_history.clone()).await;

                // Stop the spinner
                spinner_task.abort();
                print!("\r{}                    \r", " "); // Clear the spinner line
                std::io::Write::flush(&mut std::io::stdout()).unwrap();

                let response = custom_stream_to_stdout(&mut stream).await?;

                chat_history.push(Message::user(input.to_string()));
                chat_history.push(Message::assistant(response.to_string()));
            }
        }
    }

    Ok(())
}
