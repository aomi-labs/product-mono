// Core message types
export interface ChatMessage {
  id: string;
  content: string;
  sender: 'user' | 'agent';
  timestamp: Date;
  status: 'pending' | 'sent' | 'delivered' | 'error';
  type: 'text' | 'tool_call' | 'system' | 'error';
  toolCall?: ToolCallInfo;
}

// Agent message types (matching Rust backend)
export type AgentMessage = 
  | { type: 'StreamingText'; content: string }
  | { type: 'ToolCall'; name: string; args: string }
  | { type: 'Complete' }
  | { type: 'Error'; content: string }
  | { type: 'System'; content: string }
  | { type: 'McpConnected' }
  | { type: 'McpConnecting'; status: string }
  | { type: 'MissingApiKey' }
  | { type: 'Interrupted' };

// Tool execution info
export interface ToolCallInfo {
  name: string;
  args: Record<string, any>;
  result?: ToolResult;
  status: 'executing' | 'completed' | 'failed';
}

export interface ToolResult {
  content: Array<{
    type: 'text';
    text: string;
  }>;
}

// Chat state management
export interface ChatState {
  messages: ChatMessage[];
  isConnected: boolean;
  isTyping: boolean;
  connectionStatus: 'disconnected' | 'connecting' | 'connected' | 'error';
  currentStreamingMessage?: string;
  error?: string;
}

// API configuration
export interface ChatbotConfig {
  mcpServerUrl: string;
  maxMessageLength: number;
  reconnectAttempts: number;
  reconnectDelay: number;
}

// Event handlers
export interface ChatEventHandlers {
  onMessage?: (message: ChatMessage) => void;
  onConnectionChange?: (status: ChatState['connectionStatus']) => void;
  onError?: (error: string) => void;
  onTypingChange?: (isTyping: boolean) => void;
}