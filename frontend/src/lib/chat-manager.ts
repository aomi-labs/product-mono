// ChatManager.ts - Manages chat connection and state (TypeScript version)
import { ConnectionStatus, ChatManagerConfig, ChatManagerEventHandlers, ChatManagerState, Message, WalletTransaction } from './types';

export class ChatManager {
  private config: ChatManagerConfig;
  private sessionId: string;
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

    // Initialize session ID (use provided one or generate new)
    this.sessionId = config.sessionId || this.generateSessionId();

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

  private generateSessionId(): string {
    // Use crypto.randomUUID if available (modern browsers), otherwise fallback
    if (typeof crypto !== 'undefined' && crypto.randomUUID) {
      return crypto.randomUUID();
    }
    // Fallback UUID v4 implementation
    return 'xxxxxxxx-xxxx-4xxx-yxxx-xxxxxxxxxxxx'.replace(/[xy]/g, function(c) {
      const r = Math.random() * 16 | 0;
      const v = c == 'x' ? r : (r & 0x3 | 0x8);
      return v.toString(16);
    });
  }

  public getSessionId(): string {
    return this.sessionId;
  }

  public setSessionId(sessionId: string): void {
    this.sessionId = sessionId;
    // If connected, need to reconnect with new session
    if (this.state.connectionStatus === ConnectionStatus.CONNECTED) {
      this.connect();
    }
  }

  connect(): void {
    this.setConnectionStatus(ConnectionStatus.CONNECTING);

    // Close existing connection
    this.disconnect();

    try {
      this.eventSource = new EventSource(`${this.config.backendUrl}/api/chat/stream?session_id=${this.sessionId}`);

      this.eventSource.onopen = () => {
        console.log('🌐 SSE connection opened to:', `${this.config.backendUrl}/api/chat/stream?session_id=${this.sessionId}`);
        this.setConnectionStatus(ConnectionStatus.CONNECTED);
        this.reconnectAttempt = 0;
      };

      this.eventSource.onmessage = (event) => {
        try {
          // DEBUG: sleep for 5 seconds before processing
          // await new Promise(resolve => setTimeout(resolve, 5000));
          const data = JSON.parse(event.data);
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
    console.log('🚀 ChatManager.sendMessage called with:', message);
    console.log('📊 Connection status:', this.state.connectionStatus);

    if (!message || message.length > this.config.maxMessageLength) {
      console.log('❌ Message validation failed:', !message ? 'empty' : 'too long');
      this.onError(new Error('Message is empty or too long'));
      return;
    }

    if (this.state.connectionStatus !== ConnectionStatus.CONNECTED) {
      console.log('❌ Not connected to server. Status:', this.state.connectionStatus);
      this.onError(new Error('Not connected to server'));
      return;
    }

    try {
      const response = await fetch(`${this.config.backendUrl}/api/chat`, {
        method: 'POST',
        headers: {
          'Content-Type': 'application/json',
        },
        body: JSON.stringify({
          message,
          session_id: this.sessionId
        }),
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
        headers: {
          'Content-Type': 'application/json',
        },
        body: JSON.stringify({
          session_id: this.sessionId
        }),
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
        body: JSON.stringify({
          message: systemMessage,
          session_id: this.sessionId
        }),
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
        body: JSON.stringify({
          message,
          session_id: this.sessionId
        }),
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
      if (data.pending_wallet_tx === null) {
        // Clear pending transaction
        this.state.pendingWalletTx = undefined;
      } else {
        // Only process if this is a new/different transaction
        const currentTxJson = this.state.pendingWalletTx ? JSON.stringify(this.state.pendingWalletTx) : null;
        if (data.pending_wallet_tx !== currentTxJson) {
          // Parse new transaction request
          try {
            const transaction = JSON.parse(data.pending_wallet_tx);
            // console.log('🔍 Parsed NEW transaction:', transaction);
            this.state.pendingWalletTx = transaction;
            this.onWalletTransactionRequest(transaction);
          } catch (error) {
            console.error('Failed to parse wallet transaction:', error);
          }
        } else {
          // console.log('🔍 Same transaction, skipping callback');
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