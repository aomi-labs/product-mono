// Type definitions for the application

export enum ConnectionStatus {
  CONNECTED = 'connected',
  CONNECTING = 'connecting',
  DISCONNECTED = 'disconnected',
  ERROR = 'error'
}

export interface Message {
  type: 'user' | 'assistant' | 'system';
  content: string;
  timestamp?: Date;
}

export interface ChatManagerConfig {
  backendUrl: string;
  maxMessageLength: number;
  reconnectAttempts: number;
  reconnectDelay: number;
}

export interface ChatManagerEventHandlers {
  onMessage: (messages: Message[]) => void;
  onConnectionChange: (status: ConnectionStatus) => void;
  onError: (error: Error) => void;
  onTypingChange: (isTyping: boolean) => void;
}

export interface ChatManagerState {
  messages: Message[];
  connectionStatus: ConnectionStatus;
  isTyping: boolean;
  isProcessing: boolean;
}

export interface AnvilManagerConfig {
  anvilUrl: string;
  checkInterval: number;
  maxLogEntries: number;
}

export interface AnvilManagerEventHandlers {
  onStatusChange: (isConnected: boolean) => void;
  onNewLog: (log: AnvilLog) => void;
  onError: (error: Error) => void;
}

export interface AnvilLog {
  timestamp: string;
  type: 'system' | 'info' | 'block' | 'tx' | 'tx-detail' | 'error' | 'warning';
  message: string;
}

export interface ButtonProps {
  children: React.ReactNode;
  variant?: 'default' | 'tab-inactive' | 'tab-active' | 'github' | 'terminal-connect';
  onClick?: () => void;
  showIndicator?: boolean;
  disabled?: boolean;
  className?: string;
}

export interface TextSectionProps {
  type: 'ascii' | 'intro-title' | 'intro-description';
  content: string;
  options?: Record<string, any>;
}

export interface ChatContainerProps {
  messages: Message[];
  onSendMessage?: (message: string) => void;
  isTyping?: boolean;
}

export interface TerminalInputProps {
  onSendMessage?: (message: string) => void;
  placeholder?: string;
  disabled?: boolean;
}