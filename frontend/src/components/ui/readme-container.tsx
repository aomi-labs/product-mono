"use client";

import React from 'react';
import ReactMarkdown from 'react-markdown';
import type { Components } from 'react-markdown';
import remarkGfm from 'remark-gfm';

const markdownContent = `# forge-mcp

## Summary

LLM-powered terminal that orchestrates blockchain tools across multiple EVM networks. Mixes a Rust backend, MCP services, and a Next.js frontend to let agents execute transactions with wallet-safe guardrails.

## What's changed

- Native wallet dropdown for Ethereum, Base, and Arbitrum networks.
- Auto-scrolling terminal that stays pinned to assistant updates.
- Improved environment config loader and process orchestration scripts.

## Testing

- \`npm run lint\` (frontend)
- \`cargo test\` (chatbot)
- \`node test-frontend.js\` (Playwright smoke checks)

## Next PR

- [ ] Broaden wallet smoke tests to include signing flows.
- [ ] Persist chat transcripts keyed by wallet address.
- [x] Prompt agent when wallet network drifts from backend defaults.

---

> [!IMPORTANT]
> Run \`./scripts/dev.sh\` to start Anvil, the Rust backend, and the Next.js frontend together.

## Architecture

| Layer | Responsibility |
| ----- | -------------- |
| Frontend | Next.js 15 UI, wagmi wallet hooks, SSE chat stream |
| Backend | Rust API (MCP bridge, session state, transaction planner) |
| MCP | Tool catalog (Etherscan, Brave Search, 0x, Cast/Foundry) |
| Anvil | Local testnet node + forked mainnet support |

### Project layout

\`\`\`
forge-mcp/
├── chatbot/                # Rust workspace
│   ├── bin/backend/        # HTTP + SSE API
│   ├── bin/tui/            # Terminal chat client
│   └── crates/             # Shared agent + MCP libs
├── frontend/               # Next.js app (wagmi, Tailwind)
├── scripts/                # dev/prod orchestration
└── documents/              # Protocol notes and design drafts
\`\`\`

### Tool catalog

- **Etherscan** — Contract ABI + on-chain metadata
- **Brave Search** — Web lookups for real-time context
- **0x** — Swap quotes and execution routing
- **Cast (Foundry)** — Local Anvil control utilities

### Wallet integration

- wagmi + viem providers
- MetaMask connector (browser extension or embedded)
- Network switch prompts mirrored to the agent via system messages

## Deployment checklist

1. Set environment variables via \`.env.dev\`/\`.env.prod\`.
2. Provision API keys (Anthropic, Brave, Etherscan, 0x).
3. Validate config with \`python3 scripts/load_config.py\`.
4. Build frontend with \`npm run build\`.
5. Launch stack using \`./scripts/prod.sh\`.
`;

const Paragraph: Components['p'] = ({ children, className }) => {
  const classes = className ?? 'mt-5 mb-4 text-sm text-gray-400';
  return <p className={classes}>{children}</p>;
};

const CodeRenderer: Components['code'] = ({ inline, className, children, ...props }) => {
  if (inline) {
    return (
      <code className="rounded bg-slate-800 px-1.5 py-0.5 font-mono text-xs text-emerald-300" {...props}>
        {children}
      </code>
    );
  }

  const blockClasses = className ?? '';

  return (
    <pre className="mt-5 mb-4 overflow-x-auto rounded-md bg-slate-950 p-3 text-xs leading-relaxed">
      <code className={blockClasses} {...props}>
        {children}
      </code>
    </pre>
  );
};

const githubMarkdownComponents: Components = {
  h1: ({ children }) => (
    <h1 className="mb-5 border-b border-slate-800 pb-3 text-2xl font-semibold text-gray-400">{children}</h1>
  ),
  h2: ({ children }) => (
    <h2 className="mt-5 mb-4 border-b border-slate-800 pb-3 text-xl font-semibold text-gray-400">{children}</h2>
  ),
  h3: ({ children }) => <h3 className="mt-5 mb-3 text-lg font-semibold text-gray-400">{children}</h3>,
  p: Paragraph,
  ul: ({ children }) => <ul className="mb-4 ml-6 list-disc space-y-1 text-sm text-gray-400">{children}</ul>,
  ol: ({ children }) => <ol className="mb-4 ml-6 list-decimal space-y-1 text-sm text-gray-400">{children}</ol>,
  li: ({ children }) => <li className="leading-relaxed text-sm text-gray-400">{children}</li>,
  a: ({ href, children }) => (
    <a href={href} className="text-blue-400 underline hover:text-blue-300" target="_blank" rel="noreferrer">
      {children}
    </a>
  ),
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
            <Paragraph className="mt-1 text-sm leading-relaxed text-slate-100">
              {child.props.children}
            </Paragraph>
          );
        }

        if (child.type === CodeRenderer) {
          return (
            <pre className="mt-1 mb-2 overflow-x-auto rounded-md bg-slate-950 p-3 text-xs leading-relaxed">
              <code className={child.props.className}>{child.props.children}</code>
            </pre>
          );
        }

        return child;
      });

      return (
        <div className="my-4 rounded-md border border-blue-500/40 bg-blue-500/10 p-4 text-sm text-slate-100">
          <div className="mb-2 flex items-center gap-2 font-semibold uppercase tracking-wide text-blue-300">
            <span className="text-base">⚠️</span>
            {title}
          </div>
          <div className="space-y-2 text-sm">
            {restText && <p className="mt-1 leading-relaxed text-slate-100">{restText}</p>}
            {normalizedChildren}
          </div>
        </div>
      );
    }

    return (
      <blockquote className="ml-3 my-4 border-l-2 border-slate-600/70 bg-slate-900/60 px-4 py-2 text-sm text-gray-400">
        {children}
      </blockquote>
    );
  },
  table: ({ children }) => (
    <div className="mt-6 mb-4 overflow-x-auto rounded-md border border-slate-800">
      <table className="w-full border-collapse text-sm text-gray-400">{children}</table>
    </div>
  ),
  thead: ({ children }) => (
    <thead className="bg-slate-900/70 text-left text-slate-300">{children}</thead>
  ),
  tbody: ({ children }) => <tbody>{children}</tbody>,
  th: ({ children }) => (
    <th className="border-b border-slate-800 px-4 py-2 font-semibold">{children}</th>
  ),
  td: ({ children }) => <td className="border-b border-slate-800 px-4 py-2">{children}</td>,
  hr: () => <hr className="my-6 border-slate-800" />,
  input: ({ checked, ...rest }) => (
    <input
      type="checkbox"
      checked={checked}
      readOnly
      className="mr-2 h-3.5 w-3.5 rounded border-slate-500 bg-slate-800 accent-emerald-400"
      {...rest}
    />
  ),
};

export const ReadmeContainer: React.FC = () => {
  return (
    <div className="h-full overflow-y-auto bg-slate-900 px-6 py-6 scrollbar-dark">
      <div className="mx-auto w-full max-w-3xl rounded-xl border border-slate-800/80 bg-slate-950/60 p-6 shadow-lg shadow-slate-950/40">
        <div className="ml-2 mr-2 mt-2 prose prose-invert max-w-none">
          <ReactMarkdown remarkPlugins={[remarkGfm]} components={githubMarkdownComponents}>
            {markdownContent}
          </ReactMarkdown>
        </div>
      </div>
    </div>
  );
};
