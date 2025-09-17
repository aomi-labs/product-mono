import React from 'react';
import { Message } from './message';
import { TerminalInput } from './terminal-input';
import { ChatContainerProps } from '../../lib/types';

export const ChatContainer: React.FC<ChatContainerProps> = ({ messages, onSendMessage, isTyping = false }) => {
  const handleSendMessage = (message: string) => {
    if (onSendMessage) {
      onSendMessage(message);
    }
  };

  return (
    <div className="h-full bg-slate-900 flex flex-col">
      <div className="flex-1 p-4 overflow-y-auto overflow-x-hidden font-mono scrollbar-dark" id="terminal-messages-container">
        {messages.map((msg, index) => (
          <Message
            key={index}
            message={{
              type: msg.type as 'user' | 'assistant' | 'system',
              content: msg.content,
              timestamp: msg.timestamp
            }}
            isLastMessage={index === messages.length - 1}
            isTyping={index === messages.length - 1 && isTyping}
          />
        ))}
      </div>
      <TerminalInput onSendMessage={handleSendMessage} />
    </div>
  );
};