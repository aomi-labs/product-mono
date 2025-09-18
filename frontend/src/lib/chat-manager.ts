// ChatManager.ts - Manages chat connection and state (TypeScript version)
import { ConnectionStatus, ChatManagerConfig, ChatManagerEventHandlers, ChatManagerState, Message, WalletTransaction } from './types';

export class ChatManager {
  private config: ChatManagerConfig;
  private onMessage: (messages: Message[]) => void;
  private onConnectionChange: (status: ConnectionStatus) => void;
  private onError: (error: Error) => void;
  private onTypingChange: (isTyping: boolean) => void;
  private onWalletTransactionRequest: (transaction: WalletTransaction) => void;

  private state: ChatManagerState;
  private eventSource: EventSource | null = null;
  private reconnectAttempt: number = 0;

  constructor(config: Partial<ChatManagerConfig> = {}, eventHandlers: Partial<ChatManagerEventHandlers> = {}) {
    this.config = {
      backendUrl: config.backendUrl || 'http://localhost:8080',
      maxMessageLength: config.maxMessageLength || 2000,
      reconnectAttempts: config.reconnectAttempts || 5,
      reconnectDelay: config.reconnectDelay || 3000,
      ...config
    };

    // Event handlers
    this.onMessage = eventHandlers.onMessage || (() => {});
    this.onConnectionChange = eventHandlers.onConnectionChange || (() => {});
    this.onError = eventHandlers.onError || (() => {});
    this.onTypingChange = eventHandlers.onTypingChange || (() => {});
    this.onWalletTransactionRequest = eventHandlers.onWalletTransactionRequest || (() => {});

    // State
    this.state = {
      messages: [],
      connectionStatus: ConnectionStatus.DISCONNECTED,
      isTyping: false,
      isProcessing: false,
      pendingWalletTx: undefined,
    };
  }

  connect(): void {
    this.setConnectionStatus(ConnectionStatus.CONNECTING);

    // Close existing connection
    this.disconnect();

    try {
      this.eventSource = new EventSource(`${this.config.backendUrl}/api/chat/stream`);

      this.eventSource.onopen = () => {
        console.log('SSE connection opened');
        this.setConnectionStatus(ConnectionStatus.CONNECTED);
        this.reconnectAttempt = 0;
      };

      this.eventSource.onmessage = async (event) => {
        try {
          // Sleep for 5 seconds before processing
          await new Promise(resolve => setTimeout(resolve, 5000));
          const data = JSON.parse(event.data);
          console.log('🔍 Frontend received SSE data:', data);
          this.updateState(data);
        } catch (error) {
          console.error('Failed to parse SSE data:', error);
        }
      };

      this.eventSource.onerror = (error) => {
        console.error('SSE connection error:', error);
        this.handleConnectionError();
      };

    } catch (error) {
      console.error('Failed to establish SSE connection:', error);
      this.handleConnectionError();
    }
  }

  disconnect(): void {
    if (this.eventSource) {
      this.eventSource.close();
      this.eventSource = null;
    }
    this.setConnectionStatus(ConnectionStatus.DISCONNECTED);
  }

  async sendMessage(message: string): Promise<void> {
    if (!message || message.length > this.config.maxMessageLength) {
      this.onError(new Error('Message is empty or too long'));
      return;
    }

    if (this.state.connectionStatus !== ConnectionStatus.CONNECTED) {
      this.onError(new Error('Not connected to server'));
      return;
    }

    try {
      const response = await fetch(`${this.config.backendUrl}/api/chat`, {
        method: 'POST',
        headers: {
          'Content-Type': 'application/json',
        },
        body: JSON.stringify({ message }),
      });

      if (!response.ok) {
        throw new Error(`HTTP ${response.status}: ${response.statusText}`);
      }

      const data = await response.json();
      this.updateState(data);

    } catch (error) {
      console.error('Failed to send message:', error);
      this.onError(error instanceof Error ? error : new Error(String(error)));
    }
  }

  async interrupt(): Promise<void> {
    try {
      const response = await fetch(`${this.config.backendUrl}/api/interrupt`, {
        method: 'POST',
      });

      if (!response.ok) {
        throw new Error(`HTTP ${response.status}: ${response.statusText}`);
      }

      const data = await response.json();
      this.updateState(data);

    } catch (error) {
      console.error('Failed to interrupt:', error);
      this.onError(error instanceof Error ? error : new Error(String(error)));
    }
  }

  async sendNetworkSwitchRequest(networkName: string): Promise<{ success: boolean; message: string; data?: any }> {
    try {
      // Send system message asking the agent to switch networks
      const systemMessage = `Dectected user's wallet connected to ${networkName} network`;

      const response = await fetch(`${this.config.backendUrl}/api/system`, {
        method: 'POST',
        headers: {
          'Content-Type': 'application/json',
        },
        body: JSON.stringify({ message: systemMessage }),
      });

      if (!response.ok) {
        throw new Error(`HTTP ${response.status}: ${response.statusText}`);
      }

      const result = await response.json();

      return {
        success: true,
        message: `Network switch system message sent for ${networkName}`,
        data: { network: networkName }
      };

    } catch (error) {
      console.error('Failed to send network switch system message:', error);
      const errorMessage = error instanceof Error ? error.message : String(error);
      return {
        success: false,
        message: errorMessage
      };
    }
  }

  async sendTransactionResult(success: boolean, transactionHash?: string, error?: string): Promise<void> {
    const message = success
      ? `Transaction sent: ${transactionHash}`
      : `Transaction rejected by user${error ? `: ${error}` : ''}`;

    try {
      const response = await fetch(`${this.config.backendUrl}/api/system`, {
        method: 'POST',
        headers: {
          'Content-Type': 'application/json',
        },
        body: JSON.stringify({ message }),
      });

      if (!response.ok) {
        throw new Error(`HTTP ${response.status}: ${response.statusText}`);
      }

      const data = await response.json();
      this.updateState(data);

    } catch (error) {
      console.error('Failed to send transaction result:', error);
      this.onError(error instanceof Error ? error : new Error(String(error)));
    }
  }

  clearPendingTransaction(): void {
    this.state.pendingWalletTx = undefined;
  }

  private updateState(data: any): void {
    const oldState = { ...this.state };

    // Handle different data formats from backend
    if (data.messages) {
      if (Array.isArray(data.messages)) {
        // Convert backend message format to frontend format
        const convertedMessages = data.messages.map((msg: any) => ({
          type: msg.sender === 'user' ? 'user' as const :
                msg.sender === 'system' ? 'system' as const :
                'assistant' as const,
          content: msg.content,
          timestamp: msg.timestamp
        }));

        this.state.messages = convertedMessages;
      } else {
        console.error('🚨 Backend sent messages field that is not an array:', typeof data.messages, data.messages);
      }
    }

    // Handle other state updates
    if (data.isTyping !== undefined) {
      this.state.isTyping = data.isTyping;
    }

    if (data.isProcessing !== undefined) {
      this.state.isProcessing = data.isProcessing;
    }

    // Handle wallet transaction requests
    if (data.pending_wallet_tx !== undefined) {
      console.log('🔍 ChatManager received pending_wallet_tx:', data.pending_wallet_tx);
      if (data.pending_wallet_tx === null) {
        // Clear pending transaction
        console.log('🔍 Clearing pending transaction');
        this.state.pendingWalletTx = undefined;
      } else {
        // Parse new transaction request
        try {
          const transaction = JSON.parse(data.pending_wallet_tx);
          console.log('🔍 Parsed transaction:', transaction);
          this.state.pendingWalletTx = transaction;
          console.log('🔍 Calling onWalletTransactionRequest callback');
          this.onWalletTransactionRequest(transaction);
        } catch (error) {
          console.error('Failed to parse wallet transaction:', error);
        }
      }
    }

    // Check for typing changes
    if (oldState.isTyping !== this.state.isTyping) {
      this.onTypingChange(this.state.isTyping);
    }

    // Notify about message updates
    this.onMessage(this.state.messages);
  }

  private setConnectionStatus(status: ConnectionStatus): void {
    if (this.state.connectionStatus !== status) {
      this.state.connectionStatus = status;
      this.onConnectionChange(status);
    }
  }

  private handleConnectionError(): void {
    this.setConnectionStatus(ConnectionStatus.ERROR);

    if (this.reconnectAttempt < this.config.reconnectAttempts) {
      this.reconnectAttempt++;
      console.log(`Attempting to reconnect (${this.reconnectAttempt}/${this.config.reconnectAttempts})...`);

      setTimeout(() => {
        this.connect();
      }, this.config.reconnectDelay);
    } else {
      console.error('Max reconnection attempts reached');
      this.setConnectionStatus(ConnectionStatus.DISCONNECTED);
    }
  }

  getState(): ChatManagerState {
    return { ...this.state };
  }

  // Stop method for cleanup
  stop(): void {
    this.disconnect();
  }
}