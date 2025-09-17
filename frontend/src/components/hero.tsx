"use client";

import { useAccount, useConnect, useDisconnect } from "wagmi";
import { useEffect, useState } from "react";
import { Button } from "./ui/button";
import { ChatContainer } from "./ui/chat-container";
import { TextSection } from "./ui/text-section";
import { ReadmeContainer } from "./ui/readme-container";
import { AnvilLogContainer } from "./ui/anvil-log-container";
import { ConnectionStatus } from "@/lib/types";
import { ChatManager } from "@/lib/chat-manager";
import { AnvilManager } from "@/lib/anvil-manager";

// Content Data
export const content = {
  intro: {
    title: "Consumer Crypto on Natural Language",
    description: "Aomi Labs is a research and engineering group that builds agentic software. We focus on transaction pipeline automation for public blockchains, developing chain-agnostic guardrails for LLMs to generate transactions with performance, scalability, and predictability."
  },
  ascii: ` ▄▄▄·       • ▌ ▄ ·. ▪
▐█ ▀█ ▪     ·██ ▐███▪██
▄█▀▀█  ▄█▀▄ ▐█ ▌▐▌▐█·▐█·
▐█ ▪▐▌▐█▌.▐▌██ ██▌▐█▌▐█▌
 ▀  ▀  ▀█▄▀▪▀▀  █▪▀▀▀▀▀▀`,
  chat: {
    messages: [
      {
        type: 'user' as const,
        content: 'hello'
      },
      {
        type: 'assistant' as const,
        content: `Hello! I'm here to help you with software development tasks in the terminal. I can assist with:

• Running commands and scripts
• Reading and editing code files
• Searching through codebases
• Version control operations
• And much more!

I can see you have several indexed codebases available:
• foameow at /Users/cecillazhang/Code/hiring/foameow
• mcp-server at /Users/cecillazhang/Code/hiring/mcp-server
• rtg-repl at /Users/cecillazhang/Code/hiring/rtg-repl

What would you like to work on today?`
      }
    ]
  }
};

export const Hero = () => {
  const { address, isConnected } = useAccount();
  const { connect, connectors } = useConnect();
  const { disconnect } = useDisconnect();

  // State management
  const [currentTab, setCurrentTab] = useState<'chat' | 'readme' | 'anvil'>('chat');
  const [connectionStatus, setConnectionStatus] = useState<ConnectionStatus>(ConnectionStatus.DISCONNECTED);
  const [chatManager, setChatManager] = useState<ChatManager | null>(null);
  const [anvilManager, setAnvilManager] = useState<AnvilManager | null>(null);
  const [chatMessages, setChatMessages] = useState(content.chat.messages);
  const [isTyping, setIsTyping] = useState(false);
  const [anvilLogs, setAnvilLogs] = useState<any[]>([]);

  // Initialize chat and anvil managers
  useEffect(() => {
    // Initialize ChatManager
    const chatMgr = new ChatManager({
      mcpServerUrl: 'http://localhost:8080',
      maxMessageLength: 2000,
      reconnectAttempts: 5,
      reconnectDelay: 3000,
    }, {
      onMessage: (messages) => {
        setChatMessages(messages);
      },
      onConnectionChange: (status) => {
        setConnectionStatus(status);
      },
      onError: (error) => {
        console.error('Chat error:', error);
        setConnectionStatus(ConnectionStatus.ERROR);
      },
      onTypingChange: (typing) => {
        setIsTyping(typing);
      },
    });

    setChatManager(chatMgr);

    // Initialize AnvilManager
    const anvilMgr = new AnvilManager({
      anvilUrl: 'http://localhost:8545',
      checkInterval: 2000,
      maxLogEntries: 100,
    }, {
      onStatusChange: (isConnected) => {
        // Handle anvil status change
      },
      onNewLog: (log) => {
        updateAnvilLogs();
      },
      onError: (error) => {
        console.warn('Anvil error:', error);
      },
    });

    setAnvilManager(anvilMgr);

    // Start connections
    chatMgr.connect();
    anvilMgr.start();

    // Cleanup on unmount
    return () => {
      chatMgr.disconnect();
      anvilMgr.stop();
    };
  }, []);

  // Separate useEffect for scroll reveal animations to run only on client
  useEffect(() => {
    // Only run on client side to prevent hydration mismatch
    if (typeof window === 'undefined') return;

    // Initialize scroll reveal animations only on client side
    const observer = new IntersectionObserver((entries) => {
      entries.forEach(entry => {
        if (entry.isIntersecting) {
          entry.target.classList.add('animate-in');
          observer.unobserve(entry.target);
        }
      });
    }, {
      threshold: 0.1,
      rootMargin: '0px 0px -50px 0px'
    });

    // Use a small delay to ensure DOM is ready and hydration is complete
    const timeoutId = setTimeout(() => {
      document.querySelectorAll('.scroll-reveal, .slide-in-right').forEach(el => {
        observer.observe(el);
      });
    }, 100);

    return () => {
      clearTimeout(timeoutId);
      observer.disconnect();
    };
  }, []);

  // Chat message handling functions

  const handleSendMessage = (message: string) => {
    if (!chatManager || !message.trim()) return;

    chatManager.sendMessage(message.trim());
  };

  // Anvil log handling functions
  const updateAnvilLogs = () => {
    if (!anvilManager) return;

    const logs = anvilManager.getLogs();
    setAnvilLogs(logs);
  };

  const handleClearAnvilLogs = () => {
    if (!anvilManager) return;

    anvilManager.clearLogs();
    setAnvilLogs([]);
  };

  const handleConnect = () => {
    if (connectors[0]) {
      connect({ connector: connectors[0] });
    }
  };

  const handleDisconnect = () => {
    disconnect();
  };

  const switchTab = (tabName: 'chat' | 'readme' | 'anvil') => {
    setCurrentTab(tabName);
  };

  const renderTerminalContent = () => {
    switch (currentTab) {
      case 'chat':
        return <ChatContainer messages={chatMessages} onSendMessage={handleSendMessage} isTyping={isTyping} />;
      case 'readme':
        return <ReadmeContainer />;
      case 'anvil':
        return <AnvilLogContainer logs={anvilLogs} onClearLogs={handleClearAnvilLogs} />;
      default:
        return <ChatContainer messages={chatMessages} onSendMessage={handleSendMessage} isTyping={isTyping} />;
    }
  };

  const getConnectionStatusText = () => {
    if (isConnected && address) {
      return `Connected: ${address.slice(0, 6)}...${address.slice(-4)}`;
    }

    switch (connectionStatus) {
      case ConnectionStatus.CONNECTED:
        return 'Connected';
      case ConnectionStatus.CONNECTING:
        return 'Connecting...';
      case ConnectionStatus.DISCONNECTED:
        return 'Disconnected';
      case ConnectionStatus.ERROR:
        return 'Connection Error';
      default:
        return 'Disconnected';
    }
  };

  const getConnectionStatusColor = () => {
    if (isConnected) return 'text-green-400';

    switch (connectionStatus) {
      case ConnectionStatus.CONNECTED:
        return 'text-green-400';
      case ConnectionStatus.CONNECTING:
        return 'text-yellow-400';
      case ConnectionStatus.DISCONNECTED:
        return 'text-gray-400';
      case ConnectionStatus.ERROR:
        return 'text-red-400';
      default:
        return 'text-gray-400';
    }
  };

  return (
    <div id="main-container" className="w-full flex px-10 pb-5 relative bg-white flex flex-col justify-start items-center overflow-hidden">
      <div data-breakpoint="Desktop" className="self-stretch flex flex-col justify-start items-center">
        {/* Mobile Header */}
        {/* <div className="mobile-nav w-full h-20 max-w-[1500px] pt-5 pb-8 flex justify-between items-center md:hidden">
          <img src="/assets/images/aomi-logo.svg" alt="Aomi" className="h-8 w-auto" />
        </div> */}

        {/* Desktop Header */}
        <div className="desktop-nav w-full h-26 flex pt-5 pb-5 flex justify-between items-center px-4">
          <img src="/assets/images/aomi-logo.svg" alt="Aomi" className="h-15 w-auto" />
          <a href="https://github.com/aomi-labs" target="_blank" rel="noopener noreferrer" className="px-4 py-3 bg-black rounded-full flex justify-center items-center gap-0.5 hover:bg-gray-800">
            <div className="text-center justify-start pt-1 text-white text-sm font-light font-['Bauhaus_Chez_Display_2.0'] leading-tight">Github ↗</div>
          </a>
        </div>
      </div>

      <div className="w-full max-w-[1500px] flex flex-col justify-start items-center pt-10 pb-10">
        <div id="terminal-container" className="w-full max-w-[840px] h-[600px] bg-slate-900 rounded-xl shadow-[0px_16px_40px_0px_rgba(0,0,0,0.25),0px_4px_16px_0px_rgba(0,0,0,0.15)] border border-slate-700/50 overflow-hidden">
          {/* Terminal Header */}
          <div className="terminal-header bg-gray-800 px-4 py-2 flex items-center justify-between rounded-tl-2xl rounded-tr-2xl">
            <div className="flex items-center space-x-4">
              <div className="flex space-x-2">
                <div className="w-[12px] h-[12px] bg-red-500 rounded-full"></div>
                <div className="w-[12px] h-[12px] bg-yellow-500 rounded-full"></div>
                <div className="w-[12px] h-[12px] bg-green-500 rounded-full"></div>
              </div>
              {/* Tabs in Header */}
              <div className="flex items-center space-x-1">
                <Button
                  variant={currentTab === 'readme' ? 'tab-active' : 'tab-inactive'}
                  onClick={() => switchTab('readme')}
                  showIndicator={currentTab === 'readme'}
                >
                  README
                </Button>
                <Button
                  variant={currentTab === 'chat' ? 'tab-active' : 'tab-inactive'}
                  onClick={() => switchTab('chat')}
                  showIndicator={currentTab === 'chat'}
                >
                  chat
                </Button>
                <Button
                  variant={currentTab === 'anvil' ? 'tab-active' : 'tab-inactive'}
                  onClick={() => switchTab('anvil')}
                  showIndicator={currentTab === 'anvil'}
                >
                  anvil
                </Button>
              </div>
            </div>

            <div className="flex items-center space-x-2">
              <span className={`text-xs connection-status ${getConnectionStatusColor()}`}>
                {getConnectionStatusText()}
              </span>
              <Button
                variant="terminal-connect"
                onClick={isConnected ? handleDisconnect : handleConnect}
              >
                {isConnected ? 'Disconnect' : 'Connect Wallet'}
              </Button>
            </div>
          </div>

          {/* Terminal Content */}
          <div className="terminal-content h-[560px]" id="terminal-content">
            {renderTerminalContent()}
          </div>
        </div>
      </div>

      <div className="self-stretch flex flex-col justify-start items-center">
        <div className="w-full max-w-[700px] pb-28 flex flex-col justify-start items-center">
          <div className="self-stretch pt-5 pb-14 flex flex-col justify-start items-start gap-12">
            <div className="self-stretch flex flex-col justify-start items-center gap-12">
              <TextSection type="ascii" content={content.ascii} />
              <TextSection type="intro-description" content={content.intro.description} />
            </div>
          </div>
        </div>
      </div>

      <div className="w-full flex justify-center">
        <div className="w-full pt-10 pb-5 border-t border-gray-200 flex flex-col justify-end items-start gap-20 px-4">
          <div className="self-stretch inline-flex justify-start items-end gap-10">
            <img src="/assets/images/a.svg" alt="A" className="w-24 h-10 object-contain" />
            <div className="flex-1 h-4"></div>
            <div className="justify-center text-lime-800 text-1.3xl font-light font-['Bauhaus_Chez_Display_2.0'] leading-none">All Rights Reserved</div>
          </div>
        </div>
      </div>
    </div>
  );
};