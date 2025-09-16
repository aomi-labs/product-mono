import React from 'react';
import { Message as MessageType } from '../../lib/types';

interface MessageProps {
  message: MessageType;
  isLastMessage?: boolean;
  isTyping?: boolean;
}

export const Message: React.FC<MessageProps> = ({ message, isLastMessage = false, isTyping = false }) => {
  const icon = message.type === 'user' ? '👧 ➜' : message.type === 'system' ? '🔧' : '🤖';
  const iconColor = message.type === 'user' ? 'text-blue-400' : message.type === 'system' ? 'text-yellow-400' : 'text-green-400';
  const textColor = message.type === 'user' ? 'text-white' : message.type === 'system' ? 'text-yellow-300' : 'text-gray-300';

  const formatContent = (content: string, sender: string): string => {
    if (sender === 'user') {
      return `<span>${content}</span>`;
    }

    // Format assistant messages with lists and paragraphs
    const lines = content.split('\n').filter(line => line.trim());
    let formatted = '';
    let inList = false;

    for (const line of lines) {
      if (line.startsWith('•')) {
        if (!inList) {
          formatted += '<ul class="ml-4 space-y-1">';
          inList = true;
        }
        formatted += `<li class="text-gray-300">${line}</li>`;
      } else {
        if (inList) {
          formatted += '</ul>';
          inList = false;
        }
        formatted += `<p class="mb-2">${line}</p>`;
      }
    }

    if (inList) {
      formatted += '</ul>';
    }

    return formatted;
  };

  const formattedContent = formatContent(message.content, message.type);

  // Only show border if:
  // 1. It's not the last message, OR
  // 2. It's the last message AND still typing (not finished)
  const showBorder = !isLastMessage || (isLastMessage && isTyping);

  return (
    <div className="chat-array mb-4">
      <div className="flex items-start space-x-3">
        <span className={`${iconColor} ml-1 text-md`}>{icon}</span>
        <div
          className={`${textColor} text-[11px] space-y-2 py-1 px-1 leading-relaxed`}
          dangerouslySetInnerHTML={{ __html: formattedContent }}
        />
      </div>
      {showBorder && <div className="ml-8 mr-6 mt-4 border-b border-gray-700/50"></div>}
    </div>
  );
};