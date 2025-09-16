import React from 'react';

export const ReadmeContainer: React.FC = () => {
  return (
    <div className="h-full p-6 bg-slate-900 text-green-400 font-mono text-sm overflow-y-auto scrollbar-dark">
      <div className="space-y-4">
        <div className="text-lime-400 font-bold">README.md</div>
        <div className="text-gray-300">
          <p className="mb-4"># Aomi Labs</p>
          <p className="mb-4">A research and engineering group focused on building agentic software for blockchain automation.</p>
          <p className="mb-4">## Features</p>
          <ul className="ml-4 space-y-1 list-disc">
            <li>Transaction pipeline automation</li>
            <li>Chain-agnostic guardrails for LLMs</li>
            <li>Performance, scalability, and predictability</li>
            <li>Real-time blockchain monitoring</li>
            <li>Wallet integration with MetaMask</li>
          </ul>
          <p className="mt-4">## Get Started</p>
          <p className="text-blue-400">
            Click the 'Connect Wallet' button to connect your MetaMask wallet, then use the 'chat' tab to interact with our AI assistant or 'anvil' to monitor blockchain activity.
          </p>
          <p className="mt-4">## Architecture</p>
          <ul className="ml-4 space-y-1 list-disc">
            <li>Frontend: Next.js with TypeScript and Tailwind CSS</li>
            <li>Wallet Integration: wagmi and MetaMask SDK</li>
            <li>Backend: MCP (Model Context Protocol) server</li>
            <li>AI: Claude 4 Sonnet integration</li>
            <li>Blockchain: Anvil local Ethereum node support</li>
          </ul>
          <p className="mt-4">## Wallet Integration</p>
          <ul className="ml-4 space-y-1 list-disc">
            <li>Connect/disconnect MetaMask wallet</li>
            <li>View wallet address and connection status</li>
            <li>Seamless blockchain interaction</li>
            <li>Multi-chain support (Linea, Mainnet)</li>
          </ul>
        </div>
      </div>
    </div>
  );
};