"use client";

import React from 'react';
import { GithubMarkdown } from './github-markdown';

const markdownContent = `# aomi's terminal 🧚‍♀️

LLM-powered chat frontend with multi-chain support allowing generic EVM transaction executions. Built with Rust backend services, Next.js frontend, and native tools set and MCPs.

### Roadmap 

We’re pursuing a **B2B SaaS** model and partnering with protocols, ecosystems, and wallets that want LLM-powered automation in their UX or backend. We deliver tailored toolchains and agentic software through custom integrations, white-label offerings, and an data-first architecture. Ideal partners include DeFi protocols adding conversational interfaces, wallets enabling natural-language transactions, and institutional platforms seeking enterprise-grade blockchain automation, including distribution-rich projects Polymarket, Kaito, Zapper, OpenSea, and Blockworks that want to leverage their proprietary data and APIs to expand capabilities.

## 🎯 Usage Examples

### Ask Anything
\`\`\`
> What's the best pool to stake my ETH?
> How much money have I made from my LP position?
> How much shit coins does Vitalik have on Base?
\`\`\`

### Do Anything
\`\`\`
> Deposit half of my ETH into the best pool
> Sell my NFT collection X on a marketplace that supports it
> Recommend a portfolio of DeFi projects based on my holdings and deploy my capital
> Borrow as much as possible by collateralizing my Board Ape NFT
\`\`\`


## 🏗️ Architecture

\`\`\`mermaid
 graph TB
      subgraph "Frontend Layer"
          FE[Next.js Web Frontend]
          CM[Chat Manager]
      end

      subgraph "Backend Services"
          API[Rust Backend API Endpoints]
          SM[Session Manager - Arc&lt;SessionManager&gt;]
          SS[Session State - Per-User Sessions]
      end

      subgraph "Agent Processing"
          AT[Agent Thread<br/>Tokio Spawn]
          NT[Native Tools<br/>Cast, Etherscan]
          MCP_INT[MCP Integration<br/>Tool Dispatcher]
      end

      subgraph "External Services"
          MCP_EXT[External MCP Servers<br/>Custom Tools]
          CLAUDE[Claude API<br/>Anthropic LLM]
      end

      subgraph "Blockchain Layer"
          ANVIL[Anvil/Fork Networks<br/>Local Development]
          MAINNET[Live Networks<br/>Ethereum, Base, etc.]
      end

      %% Frontend connections
      FE <--> CM
      CM <--> API

      %% Backend session flow
      API <--> SM
      SM <--> SS
      SS <--> AT

      %% Agent processing
      AT <--> NT
      AT <--> MCP_INT
      AT <--> CLAUDE

      %% MCP connections
      MCP_INT <--> MCP_EXT
      NT <--> ANVIL
      NT <--> MAINNET
      MCP_EXT <--> ANVIL
      MCP_EXT <--> MAINNET

      %% Styling
      style FE fill:#1e40af,stroke:#3b82f6,stroke-width:2px,color:#fff
      style CM fill:#1e3a8a,stroke:#3b82f6,stroke-width:2px,color:#fff
      style API fill:#dc2626,stroke:#ef4444,stroke-width:2px,color:#fff
      style SM fill:#b91c1c,stroke:#ef4444,stroke-width:2px,color:#fff
      style SS fill:#991b1b,stroke:#ef4444,stroke-width:2px,color:#fff
      style AT fill:#059669,stroke:#10b981,stroke-width:2px,color:#fff
      style NT fill:#047857,stroke:#10b981,stroke-width:2px,color:#fff
      style MCP_INT fill:#065f46,stroke:#10b981,stroke-width:2px,color:#fff
      style MCP_EXT fill:#ca8a04,stroke:#eab308,stroke-width:2px,color:#fff
      style CLAUDE fill:#7c3aed,stroke:#a855f7,stroke-width:2px,color:#fff
      style ANVIL fill:#ea580c,stroke:#f97316,stroke-width:2px,color:#fff
      style MAINNET fill:#c2410c,stroke:#f97316,stroke-width:2px,color:#fff
\`\`\`

### Agent System
- **Anthropic Claude Integration**: Natural language understanding for blockchain operations
- **Session Management**: Multi-turn conversations with context preservation
- **Tool Orchestration**: Coordinates blockchain tools and external APIs

### MCP Server
- **Cast Integration**: Direct Foundry tool integration for blockchain operations
- **Multi-Network Support**: Ethereum, Polygon, Base, Arbitrum with configurable RPC endpoints
- **External APIs**:
  - **Etherscan**: Contract ABI retrieval and verification
  - **Brave Search**: Web search for real-time blockchain information

### Web Backend
- **Modular Architecture**: Separated into \`session.rs\`, \`manager.rs\`, and \`endpoint.rs\`
- **Real-time Communication**: Server-Sent Events (SSE) for streaming responses
- **Session Management**: Multi-user support with automatic cleanup

### Frontend
- **Next.js 15**: Modern React framework with Turbopack
- **Wallet Integration**: wagmi + viem for Ethereum wallet connections
- **Real-time Chat**: Streaming responses with markdown support
- **Network Switching**: Dynamic network selection and configuration

\`\`\`mermaid
sequenceDiagram
      participant Frontend
      participant SessionState as Session State
      participant AgentThread as Agent Thread
      participant MCP as MCP Server

      Note over Frontend: User sends message

      Frontend->>SessionState: POST /api/chat (message)
      SessionState->>SessionState: add_user_message()
      SessionState->>SessionState: set is_processing = true
      SessionState->>AgentThread: sender_to_llm.send(message)
      SessionState-->>Frontend: return current state

      Note over AgentThread: Process user request

      AgentThread->>AgentThread: parse message & plan
      AgentThread->>MCP: tool calls (cast, etherscan, etc.)
      MCP-->>AgentThread: tool results

      Note over MCP: Provide external capabilities

      AgentThread->>SessionState: append_assistant_message()
      SessionState->>SessionState: set is_processing = false
      SessionState-->>Frontend: SSE stream updates
\`\`\`

## 📡 API Reference

### Core Endpoints
\`POST /api/chat\` - Send message to agent
\`GET /api/state\` - Get current session state
\`GET /api/chat/stream\` - Real-time response streaming
\`POST /api/interrupt\` - Stop current operation
\`POST /api/system\` - Send system messages
\`POST /api/mcp-command\` - Execute MCP commands

### Session Management
- Sessions are automatically created and managed
- 30-minute timeout with automatic cleanup
- Multi-user support with session isolation

## 🛠️ Development

### Project Structure
\`\`\`
forge-mcp/
├── chatbot/                # Rust workspace
│   ├── bin/backend/        # Web API server
│   │   ├── src/session.rs  # Session state management
│   │   ├── src/manager.rs  # Session lifecycle management
│   │   └── src/endpoint.rs # HTTP endpoints
│   ├── crates/
│   │   ├── agent/          # Claude agent & conversation handling
│   │   └── mcp-server/     # Blockchain tools & external APIs
├── frontend/               # Next.js web application
└── documents/              # Protocol documentation
\`\`\`

### Adding New Networks
1. Add RPC URL to environment configuration
2. Networks are automatically available to the agent

### Adding New Tools
- Implement tool in \`chatbot/crates/mcp-server/src/\` and add to \`CombinedTool\` in \`combined_tool.rs\`
- Implement native tool in \`chatbot/crates/agents/src/\` and register to the agent instance

## 🔍 Advanced Features

### Multi-Network Support
- **Dynamic Switching**: Change networks mid-conversation
- **State Preservation**: Wallet addresses persist across networks
- **Configurable RPCs**: Support for any EVM-compatible network

### Real-time Streaming
- **Server-Sent Events**: Live response streaming to frontend
- **Tool Execution Visibility**: See exactly what tools are being called
- **Interruption Support**: Stop long-running operations

## 🚧 Future Enhancements

### Planned Features
- **Native Light Client**: Simulate transactions by integrating a native light client for real-time blockchain state access
- **Multi-Step Transactions**: Multi-step transaction batching through ERC-4337 Account Abstraction for complex DeFi operations
- **Persistent Conversations**: Persist conversation history based on user public key for seamless cross-session continuity
- **Stateless Agentic Threads**: Implement stateless agentic thread architecture and schedule concurrent LLM calls for improved performance

### Technical Improvements
- **Health Monitoring**: Comprehensive service health checks
- **Metrics & Observability**: Prometheus/Grafana integration
- **Docker Support**: Containerized deployment
- **Concurrent Processing**: Parallel LLM request handling for better scalability

## 🎯 Roadmap

We embrace a **B2B SaaS roadmap** and are actively seeking partnerships with existing protocols, ecosystems, and wallets who need LLM automation in their UX or backend infrastructure. Our future involves **tailoring tool sets and agentic software** to our clients' specific needs through custom integrations, white-label solutions, and API-first architecture. Target partners include DeFi protocols needing conversational interfaces, wallets enabling natural language transactions, and institutional platforms requiring enterprise blockchain automation. Partnership opportunities span SDK licensing, revenue sharing models, and co-development programs for specialized industry solutions.

## 🙏 Acknowledgments

- **Anthropic** - Claude API for natural language processing
- **Foundry** - Ethereum development framework
- **0x Protocol** - Decentralized exchange infrastructure
- **Brave Search** - Privacy-focused search API
- **Uniswap** - Decentralized trading protocol documentation
`;

export const ReadmeContainer: React.FC = () => {
  return (
    <div className="h-full overflow-y-auto bg-markdown-background px-6 py-6 scrollbar-dark">
      <div className="mx-auto w-full max-w-3xl rounded-sm border border-markdown-border bg-markdown-card p-6 shadow-lg shadow-black/40">
        <div className="ml-2 mr-2 mt-2 prose prose-invert max-w-none text-markdown-text">
          <GithubMarkdown content={markdownContent} />
        </div>
      </div>
    </div>
  );
};
