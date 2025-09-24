// WalletManager.ts - Manages wallet connection and network switching

export interface WalletManagerConfig {
  sendSystemMessage: (message: string) => Promise<void>;
}

export interface WalletManagerEventHandlers {
  onConnectionChange: (isConnected: boolean, address?: string) => void;
  onChainChange: (chainId: number, networkName: string) => void;
  onError: (error: Error) => void;
}

export interface WalletState {
  isConnected: boolean;
  address?: string;
  chainId?: number;
  networkName: string;
  hasPromptedNetworkSwitch: boolean;
}

export class WalletManager {
  private config: WalletManagerConfig;
  private onConnectionChange: (isConnected: boolean, address?: string) => void;
  private onChainChange: (chainId: number, networkName: string) => void;
  private onError: (error: Error) => void;

  private state: WalletState;

  constructor(config: WalletManagerConfig, eventHandlers: Partial<WalletManagerEventHandlers> = {}) {
    this.config = config;

    // Event handlers
    this.onConnectionChange = eventHandlers.onConnectionChange || (() => {});
    this.onChainChange = eventHandlers.onChainChange || (() => {});
    this.onError = eventHandlers.onError || (() => {});

    // Initial state
    this.state = {
      isConnected: false,
      networkName: 'testnet',
      hasPromptedNetworkSwitch: false,
    };
  }

  // Map chain ID to MCP network name
  private getChainIdToNetworkName(chainId: number): string {
    switch (chainId) {
      case 1: return 'mainnet';
      case 137: return 'polygon';
      case 42161: return 'arbitrum';
      case 8453: return 'base';
      case 1337: return 'testnet';
      case 31337: return 'testnet'; // Local testnets (Anvil)
      case 59140: return 'linea-sepolia';
      case 59144: return 'linea';
      default: return 'testnet';
    }
  }

  // Send system message to backend
  private async sendSystemMessage(message: string): Promise<void> {
    try {
      await this.config.sendSystemMessage(message);
    } catch (error) {
      this.onError(error instanceof Error ? error : new Error(String(error)));
    }
  }

  // Handle wallet connection
  async handleConnect(address: string, chainId: number): Promise<void> {
    const networkName = this.getChainIdToNetworkName(chainId);

    this.state = {
      ...this.state,
      isConnected: true,
      address,
      chainId,
      networkName,
      hasPromptedNetworkSwitch: false, // Reset on new connection
    };

    // Notify handlers
    this.onConnectionChange(true, address);
    this.onChainChange(chainId, networkName);

    // Send system message to backend
    await this.sendSystemMessage(
      `User connected wallet with address ${address} on ${networkName} network (Chain ID: ${chainId}). Ready to help with transactions.`
    );

    // Check if network switching is needed
    await this.checkAndPromptNetworkSwitch();
  }

  // Handle wallet disconnection
  async handleDisconnect(): Promise<void> {
    this.state = {
      ...this.state,
      isConnected: false,
      address: undefined,
      chainId: undefined,
      hasPromptedNetworkSwitch: false,
    };

    // Notify handlers
    this.onConnectionChange(false);

    // Send system message to backend
    await this.sendSystemMessage('Wallet disconnected. Confirm to switch to testnet');
  }

  // Handle chain change
  async handleChainChange(chainId: number): Promise<void> {
    if (!this.state.isConnected) return;

    const networkName = this.getChainIdToNetworkName(chainId);

    this.state = {
      ...this.state,
      chainId,
      networkName,
      hasPromptedNetworkSwitch: false, // Reset when chain changes
    };

    // Notify handlers
    this.onChainChange(chainId, networkName);

    // Send system message to backend
    await this.sendSystemMessage(
      `User switched wallet to ${networkName} network (Chain ID: ${chainId}).`
    );

    // Check if network switching is needed
    await this.checkAndPromptNetworkSwitch();
  }

  // Check if backend network switch is needed and prompt user
  private async checkAndPromptNetworkSwitch(): Promise<void> {
    // Don't prompt if we've already prompted for this session
    if (this.state.hasPromptedNetworkSwitch) return;

    // For now, assume backend is always on 'testnet' initially
    // In production, you might want to fetch current backend network from an endpoint
    const currentBackendNetwork = 'testnet';

    // Don't prompt if networks match
    if (this.state.networkName === currentBackendNetwork) return;

    // Mark that we've prompted
    this.state.hasPromptedNetworkSwitch = true;

    // Send system message to prompt user about network switch
    const systemMessage = `New wallet connection: ${this.state.networkName}, System configuration: ${currentBackendNetwork}. Prompt user to confirm network switch`;

    await this.sendSystemMessage(systemMessage);
  }

  // Send network switch request to backend
  async requestNetworkSwitch(networkName: string): Promise<{ success: boolean; message: string }> {
    try {
      const systemMessage = `Switch to ${networkName} network to match the user's wallet.`;
      await this.sendSystemMessage(systemMessage);

      return {
        success: true,
        message: `Network switch request sent for ${networkName}`,
      };
    } catch (error) {
      const errorMessage = error instanceof Error ? error.message : String(error);
      return {
        success: false,
        message: errorMessage,
      };
    }
  }

  // Get current wallet state
  getState(): WalletState {
    return { ...this.state };
  }

  // Update backend network (called when backend confirms network switch)
  updateBackendNetwork(networkName: string): void {
    // This would be called when the backend confirms a network switch
    // Reset the prompt flag so user can be prompted again if they switch wallet networks
    this.state = {
      ...this.state,
      networkName,
      hasPromptedNetworkSwitch: false,
    };
  }
}
