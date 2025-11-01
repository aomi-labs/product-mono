import React from 'react';
import ReactMarkdown from 'react-markdown';
import remarkGfm from 'remark-gfm';
import { Message as MessageType } from '../../lib/types';

interface MessageProps {
  message: MessageType;
  isLastMessage?: boolean;
  isTyping?: boolean;
}

export const Message: React.FC<MessageProps> = ({ message, isLastMessage = false, isTyping = false }) => {
  const icon = message.type === 'user' ? '👧 ➜' : message.type === 'system' ? '🔧' : '🤖';
  const iconColor = message.type === 'user' ? 'text-blue-400' : message.type === 'system' ? 'text-yellow-100' : 'text-green-400';
  const textColor = message.type === 'user' ? 'text-white' : message.type === 'system' ? 'text-yellow-500' : 'text-gray-300';
  const hasContent = message.content?.trim().length > 0;

  // Only show border if:
  // 1. It's not the last message, OR
  // 2. It's the last message, still typing, and not an assistant bubble (assistant typing bubble stays clean)
  const showBorder = !isLastMessage || (isLastMessage && isTyping && message.type !== 'assistant');
  // const showBorder = !isLastMessage;

  return (
    <div className="chat-array mb-4">
      <div className="flex items-start space-x-3">
        <span className={`${iconColor} ml-1 text-md`}>{icon}</span>
        <div className={`${textColor} text-[11px] py-1 px-1 leading-relaxed prose prose-invert prose-sm max-w-none`}>
          {hasContent && (
            <ReactMarkdown
              remarkPlugins={[remarkGfm]}
              components={{
                p: ({ children }) => <p className="mb-2">{children}</p>,
                ul: ({ children }) => <ul className="ml-4 space-y-1 list-disc">{children}</ul>,
                ol: ({ children }) => <ol className="ml-4 space-y-1 list-decimal">{children}</ol>,
                li: ({ children }) => <li className={textColor}>{children}</li>,
                strong: ({ children }) => <strong className="font-semibold">{children}</strong>,
                em: ({ children }) => <em className="italic">{children}</em>,
                code: ({ children }) => <code className="bg-gray-800 px-1 py-0.5 rounded text-xs font-mono">{children}</code>,
                pre: ({ children }) => <pre className="bg-gray-800 p-2 rounded text-xs font-mono overflow-x-auto">{children}</pre>,
                h1: ({ children }) => <h1 className="text-lg font-bold mb-2">{children}</h1>,
                h2: ({ children }) => <h2 className="text-md font-semibold mb-2">{children}</h2>,
                h3: ({ children }) => <h3 className="text-sm font-medium mb-1">{children}</h3>,
              }}
            >
              {message.content}
            </ReactMarkdown>
          )}

          {message.toolStream && (
            <div className={`${hasContent ? 'mt-2' : ''} text-xs`}>
              <div className="font-semibold text-gray-200">{message.toolStream.topic}</div>
              <div className="mt-1 whitespace-pre-wrap font-mono text-[10px] text-gray-500">
                {message.toolStream.content}
              </div>
            </div>
          )}
        </div>
      </div>
      {showBorder && <div className="ml-8 mr-6 mt-4 border-b border-gray-700/50"></div>}
    </div>
  );
};
