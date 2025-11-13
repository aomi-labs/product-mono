"use client";

import {
  useAccount,
  useChainId,
  useConnect,
  useDisconnect,
  useSendTransaction,
  useWaitForTransactionReceipt,
} from "wagmi";
import { useCallback, useEffect, useMemo, useRef, useState } from "react";
import Image from "next/image";
import { Button } from "./ui/button";
import { ChatContainer } from "./ui/chat-container";
import { BlogSection, TextSection } from "./ui/text-section";
import { ReadmeContainer } from "./ui/readme-container";
import { AnvilLogContainer } from "./ui/anvil-log-container";
import { WalletTransaction, Message, AnvilLog } from "@/lib/types";
import { ChatManager } from "@/lib/chat-manager";
import { AnvilManager } from "@/lib/anvil-manager";
import { WalletManager } from "@/lib/wallet-manager";
import { content, bodies, blogs } from "./content";

export const Hero = () => {
  const { address, isConnected } = useAccount();
  const { connect, connectors } = useConnect();
  const { disconnect } = useDisconnect();
  const chainId = useChainId();

  type MessageQueueItem = Message & { clientOrder: number };

  const [currentTab, setCurrentTab] = useState<"chat" | "readme" | "anvil">(
    "chat",
  );
  const [chatManager, setChatManager] = useState<ChatManager | null>(null);
  const [anvilManager, setAnvilManager] = useState<AnvilManager | null>(null);
  const [walletManager, setWalletManager] = useState<WalletManager | null>(
    null,
  );
  const [backendMessages, setBackendMessages] = useState<MessageQueueItem[]>([]);
  const [localMessages, setLocalMessages] = useState<MessageQueueItem[]>([]);
  const [anvilLogs, setAnvilLogs] = useState<AnvilLog[]>([]);
  const [memoryMode, setMemoryMode] = useState<boolean>(false);
  // const [currentBackendNetwork, setCurrentBackendNetwork] = useState<string>('testnet'); // Unused state

  // Wallet state (managed by WalletManager)
  const [walletState, setWalletState] = useState({
    isConnected: false,
    address: undefined as string | undefined,
    chainId: undefined as number | undefined,
    networkName: "testnet",
  });
  const [pendingTransaction, setPendingTransaction] =
    useState<WalletTransaction | null>(null);
  const [terminalState, setTerminalState] = useState<
    "normal" | "minimized" | "expanded" | "closed"
  >("normal");
  const [lastOpenState, setLastOpenState] =
    useState<"normal" | "expanded">("normal");
  const [isMinimizing, setIsMinimizing] = useState(false);
  const [isRestoringFromMinimize, setIsRestoringFromMinimize] = useState(false);
  const minimizeTimeoutRef = useRef<ReturnType<typeof setTimeout> | null>(null);
  const restoreTimeoutRef = useRef<ReturnType<typeof setTimeout> | null>(null);

  const messageOrderRef = useRef<Map<string, number>>(new Map());
  const nextOrderRef = useRef(0);

  const assignBackendOrders = useCallback(
    (messages: Message[]): MessageQueueItem[] => {
      const orderMap = messageOrderRef.current;
      return messages.map((msg, index) => {
        const key = `${msg.type}:${index}:${
          msg.timestamp ? msg.timestamp.getTime() : "na"
        }`;
        let order = orderMap.get(key);
        if (order === undefined) {
          order = nextOrderRef.current++;
          orderMap.set(key, order);
        }
        return { ...msg, clientOrder: order };
      });
    },
    [],
  );

  const chatMessages = useMemo(() => {
    return [...backendMessages, ...localMessages].sort(
      (a, b) => a.clientOrder - b.clientOrder,
    );
  }, [backendMessages, localMessages]);

  const handleTransactionError = useCallback(
    (error: unknown) => {
      const err = error as {
        code?: number;
        cause?: { code?: number };
        message?: string;
      };
      const isUserRejection = err.code === 4001 || err.cause?.code === 4001;

      if (isUserRejection) {
        chatManager?.sendTransactionResult(
          false,
          undefined,
          "User rejected transaction",
        );
      } else {
        chatManager?.sendTransactionResult(
          false,
          undefined,
          err.message || "Transaction failed",
        );
      }
      setPendingTransaction(null);
    },
    [chatManager],
  );

  const {
    data: hash,
    sendTransaction,
    error: sendError,
    isError: isSendError,
    isPending: isSendPending,
    reset: resetSendState,
  } = useSendTransaction();

  const { isSuccess: isConfirmed, isError: isError } =
    useWaitForTransactionReceipt({ hash });

  useEffect(() => {
    if (isSendError && sendError) {
      handleTransactionError(sendError);
    }
  }, [isSendError, sendError, handleTransactionError]);

  useEffect(() => {
    messageOrderRef.current.clear();
    nextOrderRef.current = 0;
    setBackendMessages([]);
    setLocalMessages([]);

    const backendUrl =
      process.env.NEXT_PUBLIC_BACKEND_URL || "http://localhost:8080";
    const anvilUrl =
      process.env.NEXT_PUBLIC_ANVIL_URL || "http://localhost:8545";

    const chatMgr = new ChatManager(
      {
        backendUrl,
        maxMessageLength: 2000,
        reconnectAttempts: 5,
        reconnectDelay: 3000,
      },
      {
        onMessage: (messages) => {
          setBackendMessages(assignBackendOrders(messages));
        },
        onConnectionChange: () => {},
        onError: (error) => {
          console.error("Chat error:", error);
        },
        onProcessingChange: () => {},
        onWalletTransactionRequest: (transaction) => {
          console.log(
            "🔍 Hero component received wallet transaction request:",
            transaction,
          );
          setPendingTransaction(transaction);
        },
      },
    );

    setChatManager(chatMgr);

    const anvilMgr = new AnvilManager(
      {
        anvilUrl,
        checkInterval: 2000,
        maxLogEntries: 100,
      },
      {
        onStatusChange: () => {},
        onNewLog: (log) => {
          console.log("AnvilManager new log:", log);
          setAnvilLogs((prev) => [...prev, log]);
        },
        onError: (error) => {
          console.warn("Anvil error:", error);
        },
      },
    );

    setAnvilManager(anvilMgr);

    // Initialize WalletManager
    const walletMgr = new WalletManager({
      sendSystemMessage: (message) => chatMgr.sendSystemMessage(message),
    }, {
      onConnectionChange: (isConnected, address) => {
        setWalletState(prev => ({ ...prev, isConnected, address }));
        // Update ChatManager with wallet address for session persistence
        chatMgr.setPublicKey(isConnected ? address : undefined);
      },
      onChainChange: (chainId, networkName) => {
        setWalletState(prev => ({ ...prev, chainId, networkName }));
      },
      onError: (error) => {
        console.error('Wallet error:', error);
      }
    });

    setWalletManager(walletMgr);

    chatMgr.connectSSE();
    anvilMgr.start();

    return () => {
      chatMgr.disconnectSSE();
      anvilMgr.stop();
    };
  }, [assignBackendOrders]);

  useEffect(() => {
    if (!walletManager) return;

    if (isConnected && chainId && address) {
      const addressMatches =
        walletState.address?.toLowerCase() === address.toLowerCase();
      const shouldConnect = !walletState.isConnected || !addressMatches;

      if (shouldConnect) {
        walletManager.handleConnect(address, chainId);
      }
    } else if (!isConnected && walletState.isConnected) {
      walletManager.handleDisconnect();
    }
  }, [
    isConnected,
    chainId,
    address,
    walletManager,
    walletState.isConnected,
    walletState.address,
  ]);

  useEffect(() => {
    if (!walletManager || !walletState.isConnected) return;

    if (chainId && chainId !== walletState.chainId) {
      walletManager.handleChainChange(chainId);
    }
  }, [chainId, walletManager, walletState.isConnected, walletState.chainId]);

  useEffect(() => {
    if (!pendingTransaction) {
      return;
    }

    if (!walletState.isConnected) {
      chatManager?.sendTransactionResult(false, undefined, "Wallet not connected");
      setPendingTransaction(null);
      return;
    }

    if (!sendTransaction) {
      chatManager?.sendTransactionResult(false, undefined, "Wallet hooks not ready");
      setPendingTransaction(null);
      return;
    }

    if (isSendPending) {
      return;
    }

    resetSendState?.();

    let txValue: bigint | undefined;
    let txGas: bigint | undefined;

    try {
      txValue =
        typeof pendingTransaction.value === "string" &&
        pendingTransaction.value.trim() !== ""
          ? BigInt(pendingTransaction.value)
          : undefined;
    } catch (err) {
      console.error(
        "Invalid transaction value, aborting sendTransaction",
        pendingTransaction.value,
        err,
      );
      chatManager?.sendTransactionResult(false, undefined, "Invalid transaction value");
      setPendingTransaction(null);
      return;
    }

    try {
      txGas =
        typeof pendingTransaction.gas === "string" &&
        pendingTransaction.gas.trim() !== ""
          ? BigInt(pendingTransaction.gas)
          : undefined;
    } catch (err) {
      console.error(
        "Invalid gas limit value, aborting sendTransaction",
        pendingTransaction.gas,
        err,
      );
      chatManager?.sendTransactionResult(
        false,
        undefined,
        "Invalid transaction gas limit",
      );
      setPendingTransaction(null);
      return;
    }

    const normalizedData =
      typeof pendingTransaction.data === "string"
        ? pendingTransaction.data.trim()
        : "";
    const formattedData = normalizedData
      ? ((normalizedData.startsWith("0x")
          ? normalizedData
          : `0x${normalizedData}`) as `0x${string}`)
      : undefined;

    console.log("🧾 Sending transaction payload", {
      to: pendingTransaction.to,
      data: formattedData,
      value: txValue?.toString(),
      gas: txGas?.toString(),
    });

    sendTransaction({
      to: pendingTransaction.to as `0x${string}`,
      ...(formattedData ? { data: formattedData } : {}),
      ...(txValue !== undefined ? { value: txValue } : {}),
      ...(txGas !== undefined ? { gas: txGas } : {}),
    });

    // Clear local pending state to avoid resubmitting while waiting for confirmation
    setPendingTransaction(null);
  }, [
    pendingTransaction,
    sendTransaction,
    chatManager,
    walletState.isConnected,
    isSendPending,
    resetSendState,
  ]);

  useEffect(() => {
    if (!hash || !chatManager) return;

    if (isConfirmed) {
      chatManager.sendTransactionResult(true, hash);
      setPendingTransaction(null);
    } else if (isError) {
      chatManager.sendTransactionResult(false, hash, "Transaction failed");
      setPendingTransaction(null);
    }
  }, [isConfirmed, isError, hash, chatManager]);

  useEffect(() => {
    if (typeof window === "undefined") return;

    const observer = new IntersectionObserver(
      (entries) => {
        entries.forEach((entry) => {
          if (entry.isIntersecting) {
            entry.target.classList.add("animate-in");
            observer.unobserve(entry.target);
          }
        });
      },
      {
        threshold: 0.1,
        rootMargin: "0px 0px -50px 0px",
      },
    );

    const timeoutId = setTimeout(() => {
      document
        .querySelectorAll(".scroll-reveal, .slide-in-right")
        .forEach((el) => observer.observe(el));
    }, 100);

    return () => {
      clearTimeout(timeoutId);
      observer.disconnect();
    };
  }, []);

  const handleSendMessage = (message: string) => {
    console.log("🔍 handleSendMessage called with:", message);
    if (!chatManager || !message.trim()) {
      console.log(
        "❌ Cannot send message - chatManager:",
        !!chatManager,
        "message:",
        message.trim(),
      );
      return;
    }

    console.log("✅ Sending message to ChatManager");
    chatManager.postMessageToBackend(message.trim());
  };

  const handleMemoryModeChange = (enabled: boolean) => {
    if (!chatManager) return;

    setMemoryMode(enabled);
    chatManager.setMemoryMode(enabled);
    console.log('Memory mode:', enabled ? 'enabled' : 'disabled');
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

  const switchTab = (tabName: "chat" | "readme" | "anvil") => {
    setCurrentTab(tabName);
  };

  const handleTerminalClose = () => {
    if (minimizeTimeoutRef.current) {
      clearTimeout(minimizeTimeoutRef.current);
      minimizeTimeoutRef.current = null;
    }
    if (restoreTimeoutRef.current) {
      clearTimeout(restoreTimeoutRef.current);
      restoreTimeoutRef.current = null;
    }
    setIsMinimizing(false);
    setIsRestoringFromMinimize(false);
    setLastOpenState("normal");
    setTerminalState("closed");
  };

  const handleTerminalMinimize = () => {
    if (terminalState === "minimized" || terminalState === "closed") return;
    if (minimizeTimeoutRef.current) {
      clearTimeout(minimizeTimeoutRef.current);
    }
    if (restoreTimeoutRef.current) {
      clearTimeout(restoreTimeoutRef.current);
      restoreTimeoutRef.current = null;
    }
    setIsRestoringFromMinimize(false);
    setLastOpenState(terminalState === "expanded" ? "expanded" : "normal");
    setIsMinimizing(true);
    minimizeTimeoutRef.current = setTimeout(() => {
      setIsMinimizing(false);
      setTerminalState("minimized");
      minimizeTimeoutRef.current = null;
    }, 230);
  };

  const handleTerminalExpand = () => {
    setTerminalState((prev) => {
      const next = prev === "expanded" ? "normal" : "expanded";
      setLastOpenState(next as "normal" | "expanded");
      return next;
    });
  };

  const handleRestoreFromClosed = () => {
    if (minimizeTimeoutRef.current) {
      clearTimeout(minimizeTimeoutRef.current);
      minimizeTimeoutRef.current = null;
    }
    setLastOpenState("normal");
    setTerminalState("normal");
  };

  const handleRestoreFromMinimized = () => {
    if (restoreTimeoutRef.current) {
      clearTimeout(restoreTimeoutRef.current);
    }
    setIsMinimizing(false);
    setTerminalState(lastOpenState);
    setIsRestoringFromMinimize(true);
    restoreTimeoutRef.current = setTimeout(() => {
      setIsRestoringFromMinimize(false);
      restoreTimeoutRef.current = null;
    }, 350);
  };

  const renderTerminalContent = () => {
    switch (currentTab) {
      case "chat":
        return (
          <ChatContainer
            messages={chatMessages}
            onSendMessage={handleSendMessage}
            onMemoryModeChange={handleMemoryModeChange}
            memoryMode={memoryMode}
          />
        );
      case "readme":
        return <ReadmeContainer />;
      case "anvil":
        return (
          <AnvilLogContainer
            logs={anvilLogs}
            onClearLogs={handleClearAnvilLogs}
          />
        );
      default:
        return (
          <ChatContainer
            messages={chatMessages}
            onSendMessage={handleSendMessage}
            onMemoryModeChange={handleMemoryModeChange}
            memoryMode={memoryMode}
          />
        );
    }
  };

  const getWalletStatusText = () => {
    if (walletState.isConnected && walletState.address) {
      return `Connected: ${walletState.address.slice(0, 6)}...${walletState.address.slice(-4)}`;
    }
    return "Disconnected";
  };

  const getWalletStatusColor = () => {
    if (walletState.isConnected && walletState.address) {
      return "text-green-400";
    }
    return "text-red-400";
  };

  useEffect(() => {
    return () => {
      if (minimizeTimeoutRef.current) {
        clearTimeout(minimizeTimeoutRef.current);
      }
      if (restoreTimeoutRef.current) {
        clearTimeout(restoreTimeoutRef.current);
      }
    };
  }, []);

  const isTerminalVisible =
    terminalState !== "closed" && terminalState !== "minimized";
  const terminalWrapperSpacing =
    terminalState === "closed" || terminalState === "minimized"
      ? "pt-4 pb-6"
      : "pt-10 pb-10";
  const terminalSizeClasses =
    terminalState === "expanded"
      ? "max-w-[1260px] h-[900px]"
      : "max-w-[840px] h-[600px]";
  const terminalContentHeight =
    terminalState === "expanded" ? "h-[860px]" : "h-[560px]";
  const terminalAnimationClass = isMinimizing
    ? "terminal-animate-shrink"
    : isRestoringFromMinimize
      ? "terminal-animate-pop"
      : "";

  return (
    <div
      id="main-container"
      className="w-full flex px-10 pb-5 relative bg-white flex flex-col justify-start items-center overflow-hidden"
    >
      <div
        data-breakpoint="Desktop"
        className="self-stretch flex flex-col justify-start items-center"
      >
        <div className="desktop-nav w-full h-26 flex pt-5 pb-5 flex justify-between items-center px-4">
          <Image
            src="/assets/images/aomi-logo.svg"
            alt="Aomi"
            width={200}
            height={72}
            className="h-15 w-auto"
            priority
          />
          <a
            href="https://github.com/aomi-labs"
            target="_blank"
            rel="noopener noreferrer"
            className="px-4 py-3 bg-black rounded-full flex justify-center items-center gap-0.5 hover:bg-gray-800"
          >
            <div className="text-center justify-start pt-1 text-white text-sm font-light font-['Bauhaus_Chez_Display_2.0'] leading-tight">
              Github ↗
            </div>
          </a>
        </div>
      </div>

      <div
        className={`w-full max-w-[1500px] flex flex-col justify-start items-center ${terminalWrapperSpacing}`}
      >
        {isTerminalVisible && (
          <div
            id="terminal-container"
            className={`w-full ${terminalSizeClasses} bg-gray-900 rounded-xl shadow-[0px_16px_40px_0px_rgba(0,0,0,0.25),0px_4px_16px_0px_rgba(0,0,0,0.15)] border border-gray-700/50 overflow-hidden transition-all duration-300 transform origin-bottom-left ${terminalAnimationClass}`}
          >
            <div className="terminal-header bg-[#0d1117] px-4 py-2 flex items-center justify-between rounded-tl-2xl rounded-tr-2xl border-b border-b-[0.1px] border-gray-800">
              <div className="flex items-center space-x-4">
                <div className="flex space-x-2">
                  <button
                    type="button"
                    aria-label="Close terminal"
                    onClick={handleTerminalClose}
                    className="w-[12px] h-[12px] bg-red-500 rounded-full focus:outline-none focus:ring-2 focus:ring-red-300"
                  ></button>
                  <button
                    type="button"
                    aria-label="Minimize terminal"
                    onClick={handleTerminalMinimize}
                    className="w-[12px] h-[12px] bg-yellow-500 rounded-full focus:outline-none focus:ring-2 focus:ring-yellow-300"
                  ></button>
                  <button
                    type="button"
                    aria-label="Expand terminal"
                    onClick={handleTerminalExpand}
                    className="w-[12px] h-[12px] bg-green-500 rounded-full focus:outline-none focus:ring-2 focus:ring-green-300"
                  ></button>
                </div>
                <div className="flex items-center space-x-1">
                  <Button
                    variant={
                      currentTab === "readme" ? "tab-active" : "tab-inactive"
                    }
                    onClick={() => switchTab("readme")}
                    showIndicator={currentTab === "readme"}
                  >
                    README
                  </Button>
                  <Button
                    variant={
                      currentTab === "chat" ? "tab-active" : "tab-inactive"
                    }
                    onClick={() => switchTab("chat")}
                    showIndicator={currentTab === "chat"}
                  >
                    chat
                  </Button>
                  <Button
                    variant={
                      currentTab === "anvil" ? "tab-active" : "tab-inactive"
                    }
                    onClick={() => switchTab("anvil")}
                    showIndicator={currentTab === "anvil"}
                  >
                    anvil
                  </Button>
                </div>
              </div>

              <div className="flex items-center space-x-2">
                <span
                  className={`text-xs connection-status ${getWalletStatusColor()}`}
                >
                  {getWalletStatusText()}
                </span>
                <Button
                  variant="terminal-connect"
                  onClick={
                    walletState.isConnected ? handleDisconnect : handleConnect
                  }
                >
                  {walletState.isConnected ? "Disconnect" : "Connect Wallet"}
                </Button>
              </div>
            </div>

            <div
              className={`terminal-content ${terminalContentHeight}`}
              id="terminal-content"
            >
              {renderTerminalContent()}
            </div>
          </div>
        )}

        {terminalState === "closed" && (
          <div className="py-10">
            <button
              type="button"
              onClick={handleRestoreFromClosed}
              className="px-8 py-3 rounded-full bg-gray-200 text-gray-900 text-sm font-light font-['Bauhaus_Chez_Display_2.0'] hover:bg-gray-300 transition-colors border border-gray-300"
            >
              + New Conversation
            </button>
          </div>
        )}
      </div>

      {terminalState === "minimized" && (
        <button
          type="button"
          aria-label="Restore terminal"
          onClick={handleRestoreFromMinimized}
          className="fixed bottom-6 left-6 h-14 w-14 rounded-full bg-gray-900 text-white flex items-center justify-center shadow-xl border border-gray-700 focus:outline-none focus:ring-2 focus:ring-gray-400"
        >
          <span className="text-2xl">💬</span>
        </button>
      )}

      <div className="self-stretch flex flex-col justify-start items-center">
        <div className="w-full max-w-[700px] pb-28 flex flex-col justify-start items-center">
          <div className="self-stretch pt-5 pb-14 flex flex-col justify-start items-start gap-12">
            <div className="self-stretch flex flex-col justify-start items-stretch gap-10">
              <TextSection type="ascii" content={content.ascii} />
              <TextSection
                type="intro-description"
                content={content.intro.description}
              />
              <TextSection type="ascii-sub" content={content.ascii2} />
              <div className="h-6" />

              <div className="self-stretch flex flex-col items-start">
                {bodies.map((body) => (
                  <section
                    key={body.h2}
                    className="self-stretch flex flex-col items-start gap-5"
                  >
                    <TextSection type="h2-title" content={body.h2} />
                    <ul className="self-stretch space-y-3 pl-6 pr-5 list-disc list-outside marker:text-gray-900">
                      {body.paragraphs.map((paragraph, index) => (
                        <TextSection
                          key={`${body.h2}-${index}`}
                          type="paragraph"
                          content={paragraph}
                        />
                      ))}
                    </ul>
                  </section>
                ))}
              </div>

              <div className="h-1" />
              <TextSection
                type="intro-description"
                content={content.conclusion}
              />
              <TextSection type="ascii-sub" content={content.ascii3} />
              <BlogSection blogs={blogs} className="mt-20" />
            </div>
          </div>
        </div>
      </div>

      <div className="w-full flex justify-center">
        <div className="w-full pt-10 pb-5 border-t border-gray-200 flex flex-col justify-end items-start gap-20 px-4">
          <div className="self-stretch inline-flex justify-start items-end gap-10">
            <Image
              src="/assets/images/a.svg"
              alt="A"
              width={120}
              height={40}
              className="w-24 h-10 object-contain"
            />
            <div className="flex-1 h-4"></div>
            <div className="justify-center text-lime-800 text-1.3xl font-light font-['Bauhaus_Chez_Display_2.0'] leading-none">
              All Rights Reserved
            </div>
          </div>
        </div>
      </div>
    </div>
  );
};
