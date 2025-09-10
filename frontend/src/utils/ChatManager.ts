import { ChatState, ChatMessage, ChatbotConfig, ChatEventHandlers } from '@/types';

interface BackendChatMessage {
  sender: 'user' | 'agent' | 'system';
  content: string;
  timestamp: string;
  is_streaming: boolean;
}

interface BackendStateResponse {
  messages: BackendChatMessage[];
  is_processing: boolean;
  is_loading: boolean;
  is_connecting_mcp: boolean;
  missing_api_key: boolean;
}

export class ChatManager {
  private state: ChatState;
  private config: ChatbotConfig;
  private eventHandlers: ChatEventHandlers;
  private eventSource?: EventSource;
  private reconnectTimer?: NodeJS.Timeout;

  constructor(config: ChatbotConfig, eventHandlers: ChatEventHandlers = {}) {
    this.config = config;
    this.eventHandlers = eventHandlers;
    this.state = {
      messages: [],
      isConnected: false,
      isTyping: false,
      connectionStatus: 'disconnected',
    };
  }

  // Public API
  public getState(): ChatState {
    return { ...this.state };
  }

  public async sendMessage(content: string): Promise<void> {
    const message = content.trim();
    if (!message) return;

    try {
      // Send message to backend
      const response = await fetch(`${this.config.mcpServerUrl}/api/chat`, {
        method: 'POST',
        headers: {
          'Content-Type': 'application/json',
        },
        body: JSON.stringify({ message }),
      });

      if (!response.ok) {
        throw new Error(`HTTP ${response.status}: ${response.statusText}`);
      }

      // Backend handles all agent communication, just update our state
      const data: BackendStateResponse = await response.json();
      this.updateFromBackend(data);
      
    } catch (error) {
      this.setError(error instanceof Error ? error.message : 'Failed to send message');
    }
  }

  public async interrupt(): Promise<void> {
    try {
      const response = await fetch(`${this.config.mcpServerUrl}/api/interrupt`, {
        method: 'POST',
      });

      if (!response.ok) {
        throw new Error(`HTTP ${response.status}: ${response.statusText}`);
      }

      const data: BackendStateResponse = await response.json();
      this.updateFromBackend(data);
      
    } catch (error) {
      this.setError(error instanceof Error ? error.message : 'Failed to interrupt');
    }
  }

  public connect(): void {
    if (this.state.connectionStatus === 'connected') return;
    
    this.updateConnectionStatus('connecting');
    this.checkServerHealth();
    this.establishSSEConnection();
  }

  public disconnect(): void {
    this.closeConnection();
    this.updateConnectionStatus('disconnected');
  }

  // Private methods
  private async checkServerHealth(): Promise<void> {
    try {
      const response = await fetch(`${this.config.mcpServerUrl}/health`);
      
      if (response.ok) {
        this.updateConnectionStatus('connected');
      } else {
        throw new Error('Server not ready');
      }
    } catch (error) {
      this.updateConnectionStatus('error');
      this.scheduleReconnect();
    }
  }

  private establishSSEConnection(): void {
    try {
      this.eventSource = new EventSource(`${this.config.mcpServerUrl}/api/chat/stream`);
      
      this.eventSource.onopen = () => {
        this.updateConnectionStatus('connected');
      };

      this.eventSource.onmessage = (event) => {
        try {
          const data: BackendStateResponse = JSON.parse(event.data);
          this.updateFromBackend(data);
        } catch (error) {
          console.error('Failed to parse SSE message:', error);
        }
      };

      this.eventSource.onerror = () => {
        this.updateConnectionStatus('error');
        this.scheduleReconnect();
      };
      
    } catch (error) {
      this.setError('Failed to establish SSE connection');
      this.scheduleReconnect();
    }
  }

  private updateFromBackend(backendState: BackendStateResponse): void {
    // Convert backend messages to frontend format
    const convertedMessages: ChatMessage[] = backendState.messages.map((msg, index) => ({
      id: `msg_${index}`, // Simple ID based on index
      content: msg.content,
      sender: msg.sender === 'agent' ? 'agent' : msg.sender === 'system' ? 'agent' : 'user',
      timestamp: new Date(msg.timestamp + ' GMT'), // Parse the backend timestamp
      status: msg.is_streaming ? 'pending' : 'delivered',
      type: msg.sender === 'system' ? 'system' : 'text',
    }));

    // Update state
    this.state.messages = convertedMessages;
    this.state.isTyping = backendState.is_processing;

    // Determine connection status based on backend state
    if (backendState.missing_api_key) {
      this.updateConnectionStatus('error');
      this.setError('Missing Anthropic API key');
    } else if (backendState.is_connecting_mcp) {
      this.updateConnectionStatus('connecting');
    } else {
      this.updateConnectionStatus('connected');
    }

    // Notify handlers
    this.eventHandlers.onTypingChange?.(this.state.isTyping);
    
    // Notify about new messages (just send the last one to avoid spam)
    if (convertedMessages.length > 0) {
      const lastMessage = convertedMessages[convertedMessages.length - 1];
      this.eventHandlers.onMessage?.(lastMessage);
    }
  }

  private scheduleReconnect(): void {
    if (this.reconnectTimer) {
      clearTimeout(this.reconnectTimer);
    }
    
    this.reconnectTimer = setTimeout(() => {
      if (this.state.connectionStatus !== 'connected') {
        this.connect();
      }
    }, this.config.reconnectDelay);
  }

  private closeConnection(): void {
    if (this.eventSource) {
      this.eventSource.close();
      this.eventSource = undefined;
    }
    
    if (this.reconnectTimer) {
      clearTimeout(this.reconnectTimer);
      this.reconnectTimer = undefined;
    }
  }

  private updateConnectionStatus(status: ChatState['connectionStatus']): void {
    this.state.connectionStatus = status;
    this.state.isConnected = status === 'connected';
    this.eventHandlers.onConnectionChange?.(status);
  }

  private setError(error: string): void {
    this.state.error = error;
    this.eventHandlers.onError?.(error);
  }

  // Cleanup
  public destroy(): void {
    this.closeConnection();
  }
}