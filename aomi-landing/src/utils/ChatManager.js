// ChatManager.js - Manages chat connection and state
import { ConnectionStatus } from '../types/index.js';

export class ChatManager {
  constructor(config = {}, eventHandlers = {}) {
    this.config = {
      mcpServerUrl: config.mcpServerUrl || 'http://localhost:8080',
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

    // State
    this.state = {
      messages: [],
      connectionStatus: ConnectionStatus.DISCONNECTED,
      isTyping: false,
      isProcessing: false,
    };

    this.eventSource = null;
    this.reconnectAttempt = 0;
  }

  connect() {
    this.setConnectionStatus(ConnectionStatus.CONNECTING);

    // Close existing connection
    this.disconnect();

    try {
      this.eventSource = new EventSource(`${this.config.mcpServerUrl}/api/chat/stream`);

      this.eventSource.onopen = () => {
        console.log('SSE connection opened');
        this.setConnectionStatus(ConnectionStatus.CONNECTED);
        this.reconnectAttempt = 0;
      };

      this.eventSource.onmessage = (event) => {
        try {
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

  disconnect() {
    if (this.eventSource) {
      this.eventSource.close();
      this.eventSource = null;
    }
    this.setConnectionStatus(ConnectionStatus.DISCONNECTED);
  }

  async sendMessage(message) {
    if (!message || message.length > this.config.maxMessageLength) {
      this.onError(new Error('Message is empty or too long'));
      return;
    }

    if (this.state.connectionStatus !== ConnectionStatus.CONNECTED) {
      this.onError(new Error('Not connected to server'));
      return;
    }

    try {
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

      const data = await response.json();
      this.updateState(data);

    } catch (error) {
      console.error('Failed to send message:', error);
      this.onError(error);
    }
  }

  async interrupt() {
    try {
      const response = await fetch(`${this.config.mcpServerUrl}/api/interrupt`, {
        method: 'POST',
      });

      if (!response.ok) {
        throw new Error(`HTTP ${response.status}: ${response.statusText}`);
      }

      const data = await response.json();
      this.updateState(data);

    } catch (error) {
      console.error('Failed to interrupt:', error);
      this.onError(error);
    }
  }

  updateState(newState) {
    const oldState = { ...this.state };
    this.state = { ...this.state, ...newState };

    // Check for typing changes
    if (oldState.isTyping !== this.state.isTyping) {
      this.onTypingChange(this.state.isTyping);
    }

    // Notify about message updates
    this.onMessage(this.state.messages);
  }

  setConnectionStatus(status) {
    if (this.state.connectionStatus !== status) {
      this.state.connectionStatus = status;
      this.onConnectionChange(status);
    }
  }

  handleConnectionError() {
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

  getState() {
    return { ...this.state };
  }
}