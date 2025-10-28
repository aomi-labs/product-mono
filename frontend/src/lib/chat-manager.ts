// ChatManager.ts - Manages chat connection and state (TypeScript version)
import { BackendApi, BackendMessagePayload, BackendStatePayload, normaliseReadiness } from './backend-api';
import { BackendReadiness, ConnectionStatus, ChatManagerConfig, ChatManagerEventHandlers, ChatManagerState, Message, WalletTransaction } from './types';

export class ChatManager {
  private config: ChatManagerConfig;
  private sessionId: string;
  private onMessage: (messages: Message[]) => void;
  private onConnectionChange: (status: ConnectionStatus) => void;
  private onError: (error: Error) => void;
  private onWalletTransactionRequest: (transaction: WalletTransaction) => void;
  private onProcessingChange: (isProcessing: boolean) => void;
  private onReadinessChange: (readiness: BackendReadiness) => void;
  private backend: BackendApi;

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
    this.onWalletTransactionRequest = eventHandlers.onWalletTransactionRequest || (() => {});
    this.onProcessingChange = eventHandlers.onProcessingChange || (() => {});
    this.onReadinessChange = eventHandlers.onReadinessChange || (() => {});

    // State
    this.state = {
      messages: [],
      connectionStatus: ConnectionStatus.DISCONNECTED,
      isProcessing: false,
      readiness: {
        phase: 'connecting_mcp',
      },
      pendingWalletTx: undefined,
    };

    this.backend = new BackendApi(this.config.backendUrl);
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
      this.connectSSE();
    }
  }

  connectSSE(): void {
    this.setConnectionStatus(ConnectionStatus.CONNECTING);

    // Close existing connection
    this.disconnectSSE();

    try {
      this.eventSource = new EventSource(`${this.config.backendUrl}/api/chat/stream?session_id=${this.sessionId}`);

      this.eventSource.onopen = () => {
        console.log('🌐 SSE connection opened to:', `${this.config.backendUrl}/api/chat/stream?session_id=${this.sessionId}`);
        this.setConnectionStatus(ConnectionStatus.CONNECTED);
        this.reconnectAttempt = 0;
        this.refreshState();
      };

      this.eventSource.onmessage = (event) => {
        try {
          // DEBUG: sleep for 5 seconds before processing
          // await new Promise(resolve => setTimeout(resolve, 5000));
          const data = JSON.parse(event.data);
          console.log('🔔 SSE message received:', { 
            hasMessages: !!data.messages, 
            messageCount: data.messages?.length,
            isProcessing: data.isProcessing ?? data.is_processing
          });
          this.updateChatState(data);
        } catch (error) {
          console.error('Failed to parse SSE data:', error);
        }
      };

      this.eventSource.onerror = (error) => {
        console.error('SSE connection error:', error);
        this.handleConnectionError();
        this.refreshState();
      };

    } catch (error) {
      console.error('Failed to establish SSE connection:', error);
      this.handleConnectionError();
      this.refreshState();
    }
  }

  private async refreshState(): Promise<void> {
    try {
      const data = await this.backend.fetchState(this.sessionId);
      this.updateChatState(data);
    } catch (error) {
      console.warn('Failed to refresh chat state:', error);
    }
  }

  disconnectSSE(): void {
    if (this.eventSource) {
      this.eventSource.close();
      this.eventSource = null;
    }
    this.setConnectionStatus(ConnectionStatus.DISCONNECTED);
  }

  async postMessageToBackend(message: string): Promise<void> {
    console.log('🚀 ChatManager.postMessageToBackend called with:', message);
    console.log('🐬 Current state:', {
      connectionStatus: this.state.connectionStatus,
      sessionId: this.sessionId,
      isProcessing: this.state.isProcessing,
      readiness: this.state.readiness.phase,
      messageCount: this.state.messages.length
    });

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

    // Removed readiness check - allow sending messages regardless of backend state

    try {
      const data = await this.backend.postChatMessage(this.sessionId, message);
      console.log('✅ Backend respond from /api/chat:', data);
      this.updateChatState(data);
    } catch (error) {
      console.error('Failed to send message:', error);
      this.onError(error instanceof Error ? error : new Error(String(error)));
    }
  }

  async interrupt(): Promise<void> {
    try {
      const data = await this.backend.postInterrupt(this.sessionId);
      this.updateChatState(data);
    } catch (error) {
      console.error('Failed to interrupt:', error);
      this.onError(error instanceof Error ? error : new Error(String(error)));
    }
  }

  private async postSystemMessage(message: string): Promise<BackendStatePayload> {
    const data = await this.backend.postSystemMessage(this.sessionId, message);
    this.updateChatState(data);
    return data;
  }

  async sendSystemMessage(message: string): Promise<void> {
    try {
      await this.postSystemMessage(message);
      console.log('System message sent:', message);
    } catch (error) {
      console.error('Failed to send system message:', error);
      this.onError(error instanceof Error ? error : new Error(String(error)));
    }
  }

  async sendNetworkSwitchRequest(networkName: string): Promise<{ success: boolean; message: string; data?: Record<string, unknown> }> {
    try {
      // Send system message asking the agent to switch networks
      const systemMessage = `Dectected user's wallet connected to ${networkName} network`;

      await this.postSystemMessage(systemMessage);

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
      await this.postSystemMessage(message);

    } catch (error) {
      console.error('Failed to send transaction result:', error);
      this.onError(error instanceof Error ? error : new Error(String(error)));
    }
  }

  clearPendingTransaction(): void {
    this.state.pendingWalletTx = undefined;
  }

  private updateChatState(data: BackendStatePayload): void {
    const oldState = { ...this.state };

    // Handle different data formats from backend
    if (data.messages) {
      if (Array.isArray(data.messages)) {
        // Convert backend message format to frontend format
        const convertedMessages = data.messages
          .filter((msg): msg is BackendMessagePayload => Boolean(msg))
          .map((msg) => {
            const parsedTimestamp = msg.timestamp ? new Date(msg.timestamp) : undefined;
            const timestamp = parsedTimestamp && !Number.isNaN(parsedTimestamp.valueOf()) ? parsedTimestamp : undefined;

            return {
              type: msg.sender === 'user' ? 'user' as const :
                    msg.sender === 'system' ? 'system' as const :
                    'assistant' as const,
              content: msg.content ?? '',
              timestamp,
            };
          });

        this.state.messages = convertedMessages;
      } else {
        console.error('🚨 Backend sent messages field that is not an array:', typeof data.messages, data.messages);
      }
    }

    // Removed typing state handling - always allow user input

    const processingFlag = data.isProcessing !== undefined ? data.isProcessing : data.is_processing;
    if (processingFlag !== undefined) {
      const newProcessingState = Boolean(processingFlag);
      console.log(`🐬 Processing state update: ${this.state.isProcessing} -> ${newProcessingState}, messages count: ${this.state.messages.length}`);
      this.state.isProcessing = newProcessingState;
    }

    const readiness = this.extractReadiness(data);
    if (readiness) {
      this.state.readiness = readiness;
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
            const transaction = JSON.parse(data.pending_wallet_tx) as WalletTransaction;
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

    // Removed typing change detection - always allow user input

    if (oldState.isProcessing !== this.state.isProcessing) {
      this.onProcessingChange(this.state.isProcessing);
    }

    if (
      oldState.readiness.phase !== this.state.readiness.phase ||
      oldState.readiness.detail !== this.state.readiness.detail
    ) {
      this.onReadinessChange(this.state.readiness);
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

  private extractReadiness(payload: BackendStatePayload): BackendReadiness | null {
    if (!payload) {
      return null;
    }

    const readiness = normaliseReadiness(payload.readiness);
    if (readiness) {
      return readiness;
    }

    const legacyMissing = this.resolveBoolean(payload.missingApiKey ?? payload.missing_api_key);
    if (legacyMissing) {
      return { phase: 'missing_api_key', detail: undefined };
    }

    const legacyLoading = this.resolveBoolean(payload.isLoading ?? payload.is_loading);
    if (legacyLoading) {
      return { phase: 'validating_anthropic', detail: undefined };
    }

    const legacyConnecting = this.resolveBoolean(payload.isConnectingMcp ?? payload.is_connecting_mcp);
    if (legacyConnecting) {
      return { phase: 'connecting_mcp', detail: undefined };
    }

    return null;
  }

  private resolveBoolean(value: unknown): boolean {
    if (typeof value === 'boolean') {
      return value;
    }
    if (typeof value === 'string') {
      return value.toLowerCase() === 'true';
    }
    return false;
  }

  private handleConnectionError(): void {
    this.setConnectionStatus(ConnectionStatus.ERROR);

    if (this.reconnectAttempt < this.config.reconnectAttempts) {
      this.reconnectAttempt++;
      console.log(`Attempting to reconnect (${this.reconnectAttempt}/${this.config.reconnectAttempts})...`);

      setTimeout(() => {
        this.connectSSE();
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
    this.disconnectSSE();
  }
}
