import React, { useState } from 'react';
import { AnvilLog } from '../../lib/types';

interface AnvilLogContainerProps {
  logs?: AnvilLog[];
  onClearLogs?: () => void;
}

export const AnvilLogContainer: React.FC<AnvilLogContainerProps> = ({
  logs = [],
  onClearLogs
}) => {
  const handleClearLogs = () => {
    if (onClearLogs) {
      onClearLogs();
    }
  };

  const getLogColor = (type: AnvilLog['type']): string => {
    const typeColors = {
      'system': 'text-green-400',
      'info': 'text-blue-400',
      'block': 'text-purple-400',
      'tx': 'text-yellow-400',
      'tx-detail': 'text-gray-400',
      'error': 'text-red-400',
      'warning': 'text-orange-400',
    };

    return typeColors[type] || 'text-gray-300';
  };

  const renderLogs = () => {
    if (logs.length === 0) {
      return (
        <div className="text-gray-500 text-xs">
          No logs yet. Start Anvil to see activity...
        </div>
      );
    }

    return logs.map((log, index) => (
      <div key={index} className="anvil-log-entry mb-1">
        <div className="flex items-start space-x-2">
          <span className="text-gray-500 text-xs min-w-[60px] font-mono">
            {log.timestamp}
          </span>
          <div className={`${getLogColor(log.type)} text-xs font-mono leading-relaxed`}>
            {log.message}
          </div>
        </div>
      </div>
    ));
  };

  return (
    <div className="h-full bg-slate-900 flex flex-col">
      <div
        className="flex-1 p-4 overflow-y-auto overflow-x-hidden font-mono scrollbar-dark"
        id="anvil-logs-container"
      >
        {renderLogs()}
      </div>
      <div className="px-4 py-2 border-t border-gray-700">
        <div className="flex items-center justify-between text-xs text-gray-400">
          <span>Anvil Node Monitor</span>
          <button
            id="clear-anvil-logs"
            onClick={handleClearLogs}
            className="text-gray-400 hover:text-white px-2 py-1 rounded hover:bg-gray-700"
          >
            Clear
          </button>
        </div>
      </div>
    </div>
  );
};