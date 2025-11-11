"use client";

import React, { ChangeEvent, useEffect, useMemo, useState } from 'react';
import { useAccount, useChainId, useSwitchChain } from "wagmi";
import { arbitrum, base as baseChain, mainnet as ethereumChain, optimism, polygon, sepolia } from "wagmi/chains";
import { TerminalInputProps } from '../../lib/types';

type NetworkOptionValue = 'ethereum' | 'base' | 'arbitrum' | 'optimism' | 'polygon' | 'sepolia';

const NETWORK_OPTIONS: Array<{ value: NetworkOptionValue; chainId: number }> = [
  { value: 'ethereum', chainId: ethereumChain.id },
  { value: 'base', chainId: baseChain.id },
  { value: 'arbitrum', chainId: arbitrum.id },
  { value: 'optimism', chainId: optimism.id },
  { value: 'polygon', chainId: polygon.id },
  { value: 'sepolia', chainId: sepolia.id },
];

export const TerminalInput: React.FC<TerminalInputProps> = ({
  onSendMessage,
  placeholder = 'type a message...',
  disabled = false
}) => {
  const [inputValue, setInputValue] = useState('');
  const [switchError, setSwitchError] = useState<string | null>(null);
  const [selectedNetwork, setSelectedNetwork] = useState<NetworkOptionValue>('ethereum');

  const { isConnected } = useAccount();
  const chainId = useChainId();
  const { chains, switchChainAsync, isPending: isSwitching } = useSwitchChain();

  const model = 'auto (claude 4 sonnet)';

  const availableNetworks = useMemo(() => {
    const supportedChainIds = new Set(chains.map((chain) => chain.id));
    return NETWORK_OPTIONS.filter((option) => supportedChainIds.has(option.chainId));
  }, [chains]);

  const deriveNetworkFromChainId = (id?: number): NetworkOptionValue => {
    if (!id) return 'ethereum';
    const matchedOption = NETWORK_OPTIONS.find((option) => option.chainId === id);
    if (matchedOption) return matchedOption.value;
    return 'ethereum';
  };

  useEffect(() => {
    const currentNetwork = deriveNetworkFromChainId(chainId);
    setSelectedNetwork(currentNetwork);
    setSwitchError(null);
  }, [chainId]);

  useEffect(() => {
    if (!isConnected) {
      setSelectedNetwork('ethereum');
      setSwitchError(null);
    }
  }, [isConnected]);

  const handleNetworkChange = async (event: ChangeEvent<HTMLSelectElement>) => {
    const nextValue = event.target.value as NetworkOptionValue;
    setSelectedNetwork(nextValue);
    const targetNetwork = availableNetworks.find((option) => option.value === nextValue);

    if (!targetNetwork) {
      setSwitchError('Selected network is not available in this wallet.');
      return;
    }

    try {
      await switchChainAsync({ chainId: targetNetwork.chainId });
      setSwitchError(null);
    } catch (error) {
      const message = error instanceof Error ? error.message : String(error);
      setSwitchError(message || 'Failed to switch network.');
    }
  };

  const handleKeyDown = (event: React.KeyboardEvent<HTMLInputElement>) => {
    if (event.key === 'Enter' && !event.shiftKey) {
      event.preventDefault();
      handleSendMessage();
    }
  };

  const handleSendMessage = () => {
    if (!inputValue.trim() || disabled) return;

    if (onSendMessage) {
      onSendMessage(inputValue.trim());
    }

    setInputValue('');
  };

  return (
    <div className="px-2 py-2 font-mono">
      <div className="mb-2 bg-[#30363d] border border-gray-600 rounded-md px-3 py-2 focus-within:outline-none focus-within:ring-1 focus-within:ring-blue-500 focus-within:border-blue-500">
        {/* Top icon row */}
        <div className="flex items-center space-x-3 text-xs text-gray-400 mb-3">
          <span>&gt;</span>
          <span className="text-gray-300">📍</span>
          <div className="relative">
            <select
              value={selectedNetwork}
              onChange={handleNetworkChange}
              disabled={!isConnected || isSwitching}
              className="w-40 appearance-none bg-gray-700 border border-gray-600 text-gray-100 text-xs rounded-md pl-2 pr-3 py-1 focus:outline-none focus:ring-1 focus:ring-blue-500 disabled:opacity-60"
            >
              {availableNetworks.map((option) => (
                <option key={option.value} value={option.value} className="text-gray-500">
                  {option.value}
                </option>
              ))}
            </select>
            <span className="pointer-events-none absolute pr-1 right-1.5 top-1/2 -translate-y-1/2 text-gray-400">
              ⬇️
            </span>
          </div>
          <span className="text-gray-600">|</span>
          <span>📁</span>
          <span>🎤</span>
          <span>📎</span>
          <span>📧</span>
        </div>

        {switchError && (
          <div className="mb-2 text-xs text-red-400">
            {switchError}
          </div>
        )}

        {/* Rectangular input panel */}
        <div className="mb-3">
          <input
            type="text"
            placeholder={placeholder}
            value={inputValue}
            onChange={(e) => setInputValue(e.target.value)}
            onKeyDown={handleKeyDown}
            disabled={disabled}
            className="w-full bg-[#30363d] rounded-md px-3 py-1 text-sm text-gray-300 placeholder-gray-500 text-xs focus:outline-none disabled:opacity-50"
            id="terminal-message-input"
          />
        </div>

        {/* Bottom row with model selector */}
        <div className="flex items-center justify-between">
          <div className="flex items-center space-x-2">
            <span className="text-gray-400 text-xs">{model}</span>
            <button className="px-1 py-0.5 rounded-md hover:bg-gray-700 text-xs">⬇️</button>
          </div>
        </div>
      </div>
    </div>
  );
};
