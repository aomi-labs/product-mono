"use client";

import React, { createContext, useContext, useEffect, useRef, useState } from 'react';
import ReactMarkdown from 'react-markdown';
import type { Components } from 'react-markdown';
import remarkGfm from 'remark-gfm';

// Custom Mermaid component with dynamic import
const MermaidDiagram: React.FC<{ code: string }> = ({ code }) => {
  const containerRef = useRef<HTMLDivElement>(null);
  const diagramIdRef = useRef<string>(`mermaid-${Math.random().toString(36).slice(2)}`);
  const [isLoaded, setIsLoaded] = useState(false);

  useEffect(() => {
    let cancelled = false;

    const renderMermaid = async () => {
      const container = containerRef.current;
      if (typeof window === 'undefined' || !container) {
        return;
      }

      try {
        setIsLoaded(false);

        const mermaidModule = await import('mermaid');
        const mermaid = mermaidModule.default;
        mermaid.initialize({
          startOnLoad: false,
          theme: 'dark',
          themeVariables: {
            primaryColor: '#1e293b',
            primaryTextColor: '#e2e8f0',
            primaryBorderColor: '#475569',
            lineColor: '#64748b',
            secondaryColor: '#334155',
            tertiaryColor: '#0f172a',
            background: '#0f172a',
            mainBkg: '#1e293b',
            secondBkg: '#334155',
            tertiaryBkg: '#475569'
          }
        });

        const { svg } = await mermaid.render(diagramIdRef.current, code.trim());

        if (cancelled || !containerRef.current) return;

        containerRef.current.innerHTML = svg;
        setIsLoaded(true);
      } catch (error) {
        console.error('Mermaid rendering error:', error);
        if (!containerRef.current) return;

        containerRef.current.innerHTML = '';
        const pre = document.createElement('pre');
        pre.className = 'bg-gray-800 p-4 rounded text-xs text-gray-300 overflow-x-auto';
        const codeEl = document.createElement('code');
        codeEl.textContent = code;
        pre.appendChild(codeEl);
        containerRef.current.appendChild(pre);
        setIsLoaded(true);
      }
    };

    renderMermaid();

    return () => {
      cancelled = true;
    };
  }, [code]);

  return (
    <div className="mt-5 mb-5 mx-10">
      <div
        ref={containerRef}
        className={isLoaded ? 'mx-auto max-w-full overflow-x-auto' : 'w-full animate-pulse rounded bg-gray-800'}
      />
    </div>
  );
};

const CodeBlockContext = createContext(false);

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
      AgentThread->>AgentThread: generate LLM response
      AgentThread->>SessionState: sender_to_ui.send(response)

      Note over SessionState: Update state with agent response

      SessionState->>SessionState: update assistant message
      SessionState->>SessionState: set is_processing = false

      loop SSE Updates
          Frontend->>SessionState: GET /api/chat/stream
          SessionState->>SessionState: update_state()
          SessionState-->>Frontend: current state (messages, processing status)
      end
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

- **Native Light Client**: Simulate transactions by integrating a native light client against chain head to ensure validity
- **Multi-Step Transactions**: Multi-step transaction batching through ERC-4337 Account Abstraction for complex DeFi operations
- **Persistent Conversations**: Persist conversation history based on user public key for seamless cross-session continuity
- **Stateless Agentic Threads**: Implement stateless agentic thread architecture and schedule concurrent LLM calls for improved performance

`;

const Paragraph: Components['p'] = ({ children, className }) => {
  const classes = className ?? 'mt-5 mb-4 text-[13px] text-gray-300';
  return <p className={classes}>{children}</p>;
};
const hashString = (input: string): string => {
  let hash = 0;
  for (let i = 0; i < input.length; i += 1) {
    hash = (hash * 31 + input.charCodeAt(i)) | 0;
  }
  return Math.abs(hash).toString(16);
};

const CodeRenderer: Components['code'] = ({ className, children, node, ...props }) => {
  const isInCodeBlock = useContext(CodeBlockContext);
  const codeValue = String(children ?? '').replace(/\n$/, '');
  const classList = Array.isArray((node as { properties?: { className?: string[] } })?.properties?.className)
    ? ((node as { properties?: { className?: string[] } }).properties?.className ?? [])
    : [];
  const isMermaid = classList.includes('language-mermaid');

  if (isMermaid) {
    const stableKey = `mermaid-${hashString(codeValue)}`;
    return <MermaidDiagram key={stableKey} code={codeValue} />;
  }

  if (isInCodeBlock) {
    const combined = className ? `${className} text-gray-300` : 'text-gray-300';
    return (
      <code className={combined} {...props}>
        {children}
      </code>
    );
  }

  return (
    <code className="inline rounded-sm bg-[#161b22] px-1.5 py-0.5 font-mono text-[12px] text-green-300" {...props}>
      {children}
    </code>
  );
};

const PreRenderer: Components['pre'] = ({ children, node: _node, ...props }) => {
  const childArray = React.Children.toArray(children);
  const singleChild = childArray.length === 1 ? childArray[0] : null;

  if (React.isValidElement(singleChild) && singleChild.type === MermaidDiagram) {
    return <>{singleChild}</>;
  }

  return (
    <CodeBlockContext.Provider value={true}>
      <pre
        className="mt-5 mb-4 overflow-x-auto rounded-sm bg-[#161b22] p-3 text-[12px] text-gray-300 leading-relaxed"
        {...props}
      >
        {childArray}
      </pre>
    </CodeBlockContext.Provider>
  );
};

const githubMarkdownComponents: Components = {
  h1: ({ children }) => (
    <h1 className="mb-5 border-b border-gray-500/40 pb-3 text-2xl font-semibold text-gray-300">{children}</h1>
  ),
  h2: ({ children }) => (
    <h2 className="mt-6 mb-4 border-b border-gray-500/40 pb-3 text-xl font-semibold text-gray-300">{children}</h2>
  ),
  h3: ({ children }) => <h3 className="mt-5 mb-3 text-[17px] font-semibold text-gray-300">{children}</h3>,
  p: Paragraph,
  ul: ({ children }) => <ul className="mb-4 ml-6 list-disc space-y-1 text-[13px] text-gray-300">{children}</ul>,
  ol: ({ children }) => <ol className="mb-4 ml-6 list-decimal space-y-1 text-[13px] text-gray-300">{children}</ol>,
  li: ({ children }) => <li className="leading-relaxed text-[13px] text-gray-300">{children}</li>,
  a: ({ href, children }) => (
    <a href={href} className="text-blue-400 underline hover:text-blue-300" target="_blank" rel="noreferrer">
      {children}
    </a>
  ),
  pre: PreRenderer,
  code: CodeRenderer,
  blockquote: ({ children }) => {
    const childArray = React.Children.toArray(children);

    const extractText = (node: React.ReactNode): string => {
      if (typeof node === 'string') return node;
      if (React.isValidElement(node)) {
        return React.Children.toArray(node.props.children).map(extractText).join('');
      }
      return '';
    };

    const firstChildText = extractText(childArray[0] ?? '').trim();
    const calloutMatch = firstChildText.match(/^\[!([A-Z]+)]\s*(.*)$/i);

    if (calloutMatch) {
      const [, rawType, restText] = calloutMatch;
      const title = rawType.charAt(0).toUpperCase() + rawType.slice(1).toLowerCase();
      const remainingChildren = childArray.slice(1);

      const normalizedChildren = React.Children.map(remainingChildren, (child) => {
        if (!React.isValidElement(child)) {
          return child;
        }

        if (child.type === Paragraph) {
          return (
            <Paragraph className="mt-1 text-[12px] leading-relaxed text-slate-100">
              {child.props.children}
            </Paragraph>
          );
        }

        if (child.type === CodeRenderer) {
          return (
            <pre className="mt-1 mb-2 overflow-x-auto rounded-sm bg-gray-950 p-3 text-xs leading-relaxed">
              <code className={child.props.className}>{child.props.children}</code>
            </pre>
          );
        }

        return child;
      });

      return (
        <div className="my-4 rounded-sm border border-blue-500/40 bg-blue-500/10 p-4 text-[12px] text-slate-100">
          <div className="mb-2 flex items-center gap-2 font-semibold uppercase tracking-wide text-blue-300">
            <span className="text-base">⚠️</span>
            {title}
          </div>
          <div className="space-y-2 text-[12px]">
            {restText && <p className="mt-1 leading-relaxed text-slate-100">{restText}</p>}
            {normalizedChildren}
          </div>
        </div>
      );
    }

    return (
      <blockquote className="ml-3 my-4 border-l-2 border-gray-600/70 bg-gray-900/60 px-4 py-2 text-[12px] text-gray-300">
        {children}
      </blockquote>
    );
  },
  table: ({ children }) => (
    <div className="mt-6 mb-4 overflow-x-auto rounded-sm border border-gray-500/40">
      <table className="w-full border-collapse text-[12px] text-gray-300">{children}</table>
    </div>
  ),
  thead: ({ children }) => (
    <thead className="bg-gray-900/70 text-left text-slate-300">{children}</thead>
  ),
  tbody: ({ children }) => <tbody>{children}</tbody>,
  th: ({ children }) => (
    <th className="border-b border-gray-500/40 px-4 py-2 font-semibold">{children}</th>
  ),
  td: ({ children }) => <td className="border-b border-gray-500/40 px-4 py-2">{children}</td>,
  hr: () => <hr className="my-6 border-gray-500/40" />,
  input: ({ checked, ...rest }) => (
    <input
      type="checkbox"
      checked={checked}
      readOnly
      className="mr-2 h-3.5 w-3.5 rounded-sm border-gray-500/40 bg-gray-800 accent-emerald-400"
      {...rest}
    />
  ),
};

export const ReadmeContainer: React.FC = () => {
  return (
    <div className="h-full overflow-y-auto bg-[#161b22] px-6 py-6 scrollbar-dark">
      <div className="mx-auto w-full max-w-3xl rounded-sm border border-[0.5px] border-gray-700 bg-[#2c3035] p-6 shadow-lg shadow-slate-950/40">
        <div className="ml-2 mr-2 mt-2 prose prose-invert max-w-none">
          <ReactMarkdown remarkPlugins={[remarkGfm]} components={githubMarkdownComponents}>
            {markdownContent}
          </ReactMarkdown>
        </div>
      </div>
    </div>
  );
};
