import { ChatManager } from '@/utils/ChatManager';
import { ChatMessage, ChatbotConfig } from '@/types';

export class ChatComponent {
  private container: HTMLElement;
  private chatManager: ChatManager;
  private messagesContainer!: HTMLElement;
  private inputField!: HTMLInputElement;
  private sendButton!: HTMLButtonElement;
  private statusIndicator!: HTMLElement;

  constructor(container: HTMLElement, config?: Partial<ChatbotConfig>) {
    this.container = container;
    
    const defaultConfig: ChatbotConfig = {
      mcpServerUrl: 'http://localhost:8080',
      maxMessageLength: 2000,
      reconnectAttempts: 5,
      reconnectDelay: 3000,
      ...config,
    };

    this.chatManager = new ChatManager(defaultConfig, {
      onMessage: this.handleNewMessage.bind(this),
      onConnectionChange: this.handleConnectionChange.bind(this),
      onError: this.handleError.bind(this),
      onTypingChange: this.handleTypingChange.bind(this),
    });

    this.init();
  }

  private init(): void {
    this.render();
    this.attachEventListeners();
    this.chatManager.connect();
  }

  private render(): void {
    this.container.innerHTML = `
      <div class="chat-container">
        <div class="chat-header">
          <div class="chat-title">
            <span class="chat-logo">🤖</span>
            <h3>EVM Agent</h3>
          </div>
          <div class="connection-status" id="connection-status">
            <span class="status-indicator"></span>
            <span class="status-text">Connecting...</span>
          </div>
        </div>
        
        <div class="messages-container" id="messages-container">
          <div class="welcome-message">
            <div class="message agent-message">
              <div class="message-content">
                <p>👋 Hi! I'm your Ethereum agent. I can help you with:</p>
                <ul>
                  <li>Check account balances and transaction history</li>
                  <li>Make contract calls and transactions</li>
                  <li>Search Uniswap documentation</li>
                  <li>Get swap prices and token information</li>
                </ul>
                <p>What would you like to do?</p>
              </div>
            </div>
          </div>
        </div>
        
        <div class="typing-indicator" id="typing-indicator" style="display: none;">
          <div class="typing-dots">
            <span></span>
            <span></span>
            <span></span>
          </div>
          <span class="typing-text">Agent is thinking...</span>
        </div>
        
        <div class="chat-input">
          <div class="input-group">
            <input 
              type="text" 
              id="message-input" 
              placeholder="Ask about Ethereum, contracts, or Uniswap..."
              maxlength="2000"
              autocomplete="off"
            />
            <button id="send-button" type="button" disabled>
              <span class="send-icon">📤</span>
            </button>
          </div>
          <div class="input-footer">
            <div class="character-count">
              <span id="char-count">0</span>/2000
            </div>
          </div>
        </div>
      </div>
    `;

    // Get references to key elements
    this.messagesContainer = this.container.querySelector('#messages-container')!;
    this.inputField = this.container.querySelector('#message-input')!;
    this.sendButton = this.container.querySelector('#send-button')!;
    this.statusIndicator = this.container.querySelector('#connection-status')!;
  }

  private attachEventListeners(): void {
    // Input field events
    this.inputField.addEventListener('input', this.handleInputChange.bind(this));
    this.inputField.addEventListener('keypress', this.handleKeyPress.bind(this));
    
    // Send button
    this.sendButton.addEventListener('click', this.handleSendMessage.bind(this));
    
    // Auto-resize input (optional enhancement)
    this.inputField.addEventListener('input', this.updateCharacterCount.bind(this));
  }

  private handleInputChange(): void {
    const hasText = this.inputField.value.trim().length > 0;
    const isConnected = this.chatManager.getState().isConnected;
    this.sendButton.disabled = !hasText || !isConnected;
  }

  private handleKeyPress(event: KeyboardEvent): void {
    if (event.key === 'Enter' && !event.shiftKey) {
      event.preventDefault();
      this.handleSendMessage();
    }
  }

  private async handleSendMessage(): Promise<void> {
    const message = this.inputField.value.trim();
    if (!message || this.sendButton.disabled) return;

    // Clear input
    this.inputField.value = '';
    this.updateCharacterCount();
    this.handleInputChange();

    // Send message
    try {
      await this.chatManager.sendMessage(message);
    } catch (error) {
      console.error('Failed to send message:', error);
    }
  }

  private handleNewMessage(message: ChatMessage): void {
    this.addMessageToUI(message);
    this.scrollToBottom();
  }

  private handleConnectionChange(status: string): void {
    const statusElement = this.statusIndicator;
    const statusText = statusElement.querySelector('.status-text')!;
    // const statusIndicator = statusElement.querySelector('.status-indicator')!;
    
    statusElement.className = `connection-status status-${status}`;
    
    switch (status) {
      case 'connected':
        statusText.textContent = 'Connected';
        break;
      case 'connecting':
        statusText.textContent = 'Connecting...';
        break;
      case 'disconnected':
        statusText.textContent = 'Disconnected';
        break;
      case 'error':
        statusText.textContent = 'Connection Error';
        break;
    }
    
    this.handleInputChange();
  }

  private handleError(error: string): void {
    this.addSystemMessage(`Error: ${error}`, 'error');
  }

  private handleTypingChange(isTyping: boolean): void {
    const typingIndicator = this.container.querySelector('#typing-indicator')!;
    (typingIndicator as HTMLElement).style.display = isTyping ? 'flex' : 'none';
    
    if (isTyping) {
      this.scrollToBottom();
    }
  }

  private addMessageToUI(message: ChatMessage): void {
    const messageElement = this.createMessageElement(message);
    
    // Remove welcome message if it exists
    const welcomeMessage = this.messagesContainer.querySelector('.welcome-message');
    if (welcomeMessage) {
      welcomeMessage.remove();
    }
    
    this.messagesContainer.appendChild(messageElement);
  }

  private createMessageElement(message: ChatMessage): HTMLElement {
    const messageDiv = document.createElement('div');
    messageDiv.className = `message ${message.sender}-message`;
    messageDiv.dataset.messageId = message.id;
    
    let content = '';
    
    if (message.type === 'tool_call' && message.toolCall) {
      content = `
        <div class="tool-call">
          <div class="tool-name">🔧 ${message.toolCall.name}</div>
          <div class="tool-status status-${message.toolCall.status}">
            ${message.toolCall.status}
          </div>
          ${message.toolCall.result ? `
            <div class="tool-result">
              ${message.toolCall.result.content.map(c => c.text).join('')}
            </div>
          ` : ''}
        </div>
      `;
    } else {
      content = `<div class="message-content">${this.formatMessageContent(message.content)}</div>`;
    }
    
    messageDiv.innerHTML = `
      ${content}
      <div class="message-meta">
        <span class="message-time">${this.formatTime(message.timestamp)}</span>
        ${message.sender === 'user' ? `
          <span class="message-status status-${message.status}"></span>
        ` : ''}
      </div>
    `;
    
    return messageDiv;
  }

  private addSystemMessage(content: string, _type: 'info' | 'error' = 'info'): void {
    const message: ChatMessage = {
      id: `system_${Date.now()}`,
      content,
      sender: 'agent',
      timestamp: new Date(),
      status: 'delivered',
      type: 'system',
    };
    
    this.addMessageToUI(message);
  }

  private formatMessageContent(content: string): string {
    // Basic formatting - could be enhanced with markdown parser
    return content
      .replace(/\n/g, '<br>')
      .replace(/`([^`]+)`/g, '<code>$1</code>')
      .replace(/\*\*([^*]+)\*\*/g, '<strong>$1</strong>');
  }

  private formatTime(date: Date): string {
    return date.toLocaleTimeString([], { hour: '2-digit', minute: '2-digit' });
  }

  private updateCharacterCount(): void {
    const charCount = this.container.querySelector('#char-count')!;
    charCount.textContent = this.inputField.value.length.toString();
  }

  private scrollToBottom(): void {
    this.messagesContainer.scrollTop = this.messagesContainer.scrollHeight;
  }

  // Public API
  public destroy(): void {
    this.chatManager.destroy();
  }

  public getState() {
    return this.chatManager.getState();
  }
}