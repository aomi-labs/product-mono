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
  sessionId?: string; // Optional for external session management
}

export interface ChatManagerEventHandlers {
  onMessage: (messages: Message[]) => void;
  onConnectionChange: (status: ConnectionStatus) => void;
  onError: (error: Error) => void;
  onWalletTransactionRequest?: (transaction: WalletTransaction) => void;
  onProcessingChange?: (isProcessing: boolean) => void;
  onReadinessChange?: (readiness: BackendReadiness) => void;
}

export interface ChatManagerState {
  messages: Message[];
  connectionStatus: ConnectionStatus;
  isProcessing: boolean;
  readiness: BackendReadiness;
  pendingWalletTx?: WalletTransaction;
}

export interface BackendReadiness {
  phase: 'connecting_mcp' | 'validating_anthropic' | 'ready' | 'missing_api_key' | 'error';
  detail?: string;
}

export interface WalletTransaction {
  to: string;
  value: string;
  data: string;
  gas?: string;
  description: string;
  timestamp: string;
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
  type: 'ascii' | 'intro-title' | 'intro-description' | 'h2-title' | 'paragraph' | 'ascii-sub' | 'headline';
  content: string;
  options?: Record<string, unknown>;
}

export interface BlogEntry {
  slug: string;
  title: string;
  description: string;
  imageSrc: string;
  imageAlt: string;
  eyebrow?: string;
  publishedAt?: string;
  cta?: {
    label: string;
    href: string;
  };
  body?: string;
}

export interface ChatContainerProps {
  messages: Message[];
  onSendMessage?: (message: string) => void;
}

export interface TerminalInputProps {
  onSendMessage?: (message: string) => void;
  placeholder?: string;
  disabled?: boolean;
}
