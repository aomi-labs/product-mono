import React, { useEffect, useRef } from 'react';
import { Message } from './message';
import { TerminalInput } from './terminal-input';
import { ChatContainerProps } from '../../lib/types';

export const ChatContainer: React.FC<ChatContainerProps> = ({ messages, onSendMessage, isTyping = false, isBusy = false }) => {
  const handleSendMessage = (message: string) => {
    if (onSendMessage) {
      onSendMessage(message);
    }
  };

  const showTypingIndicator = isTyping || isBusy;
  const messagesRef = useRef<HTMLDivElement | null>(null);
  const autoScrollEnabledRef = useRef(true);
  const previousMessageCountRef = useRef(messages.length);
  const previousLastMessageRef = useRef<{ type: 'user' | 'assistant' | 'system'; content: string } | null>(null);

  useEffect(() => {
    const container = messagesRef.current;
    if (!container) return;

    const handleScroll = () => {
      const distanceFromBottom = container.scrollHeight - container.scrollTop - container.clientHeight;
      const isAtBottom = distanceFromBottom <= 40;
      autoScrollEnabledRef.current = isAtBottom;
    };

    container.addEventListener('scroll', handleScroll);
    handleScroll();

    return () => {
      container.removeEventListener('scroll', handleScroll);
    };
  }, []);

  useEffect(() => {
    const container = messagesRef.current;
    if (!container) return;

    const lastMessage = messages[messages.length - 1];
    const hasNewMessage = messages.length > previousMessageCountRef.current;
    const isAgentMessage = Boolean(lastMessage && (lastMessage.type === 'assistant' || lastMessage.type === 'system'));
    const previousLast = previousLastMessageRef.current;
    const hasUpdatedAgentMessage = Boolean(
      lastMessage &&
      previousLast &&
      isAgentMessage &&
      previousLast.type === lastMessage.type &&
      previousLast.content !== lastMessage.content
    );
    const shouldScrollForMessage = hasNewMessage && isAgentMessage;
    const shouldScrollForTyping = isTyping && isAgentMessage;

    previousMessageCountRef.current = messages.length;
    previousLastMessageRef.current = lastMessage
      ? { type: lastMessage.type as 'user' | 'assistant' | 'system', content: lastMessage.content }
      : null;

    if (!autoScrollEnabledRef.current) return;
    if (!shouldScrollForMessage && !shouldScrollForTyping && !hasUpdatedAgentMessage) return;

    requestAnimationFrame(() => {
      if (!messagesRef.current) return;
      messagesRef.current.scrollTop = messagesRef.current.scrollHeight;
    });
  }, [messages, isTyping]);

  return (
    <div className="h-full bg-[#161b22] flex flex-col">
      <div
        ref={messagesRef}
        className="flex-1 p-4 overflow-y-auto overflow-x-hidden font-mono scrollbar-dark"
        id="terminal-messages-container"
      >
        {messages.map((msg, index) => (
          <Message
            key={index}
            message={{
              type: msg.type as 'user' | 'assistant' | 'system',
              content: msg.content,
              timestamp: msg.timestamp
            }}
            isLastMessage={index === messages.length - 1}
            isTyping={index === messages.length - 1 && showTypingIndicator}
          />
        ))}
      </div>
      <TerminalInput onSendMessage={handleSendMessage} disabled={isBusy} />
    </div>
  );
};
