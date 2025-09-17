import React, { useState } from 'react';
import { TerminalInputProps } from '../../lib/types';

export const TerminalInput: React.FC<TerminalInputProps> = ({
  onSendMessage,
  placeholder = 'type a message...',
  disabled = false
}) => {
  const [inputValue, setInputValue] = useState('');

  const prompt = '📁 ~ hello ❯';
  const model = 'auto (claude 4 sonnet)';

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
      <div className="mb-2 bg-slate-800 border border-slate-600 rounded-md px-3 py-2 focus-within:outline-none focus-within:ring-1 focus-within:ring-blue-500 focus-within:border-blue-500">
        {/* Top icon row */}
        <div className="flex items-center space-x-3 text-xs text-gray-400 mb-3">
          <span>&gt;</span>
          <span className="text-blue-400">🔧</span>
          <span className="text-gray-300">📍</span>
          <span>Auto</span>
          <span className="text-gray-600">|</span>
          <span>📁</span>
          <span>🎤</span>
          <span>📎</span>
          <span>📧</span>
        </div>

        {/* Rectangular input panel */}
        <div className="mb-3">
          <input
            type="text"
            placeholder={placeholder}
            value={inputValue}
            onChange={(e) => setInputValue(e.target.value)}
            onKeyDown={handleKeyDown}
            disabled={disabled}
            className="w-full bg-slate-800 rounded-md px-3 py-1 text-sm text-gray-300 placeholder-gray-500 text-xs focus:outline-none disabled:opacity-50"
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