// WalletManager.js - Manages wallet connection and network detection
export class WalletManager {
  constructor(eventHandlers = {}) {
    // Event handlers
    this.onWalletConnected = eventHandlers.onWalletConnected || (() => {});
    this.onWalletDisconnected = eventHandlers.onWalletDisconnected || (() => {});
    this.onNetworkChanged = eventHandlers.onNetworkChanged || (() => {});
    this.onError = eventHandlers.onError || (() => {});

    // State
    this.state = {
      isConnected: false,
      account: null,
      chainId: null,
      networkName: null,
    };

    // Bind methods
    this.handleAccountsChanged = this.handleAccountsChanged.bind(this);
    this.handleChainChanged = this.handleChainChanged.bind(this);
    this.handleDisconnect = this.handleDisconnect.bind(this);

    // Initialize MetaMask listeners if available
    this.setupEventListeners();
  }

  setupEventListeners() {
    if (typeof window.ethereum !== 'undefined') {
      // Listen for account changes
      window.ethereum.on('accountsChanged', this.handleAccountsChanged);
      
      // Listen for network changes
      window.ethereum.on('chainChanged', this.handleChainChanged);
      
      // Listen for disconnect
      window.ethereum.on('disconnect', this.handleDisconnect);
    }
  }

  async connect() {
    try {
      if (typeof window.ethereum === 'undefined') {
        throw new Error('MetaMask is not installed');
      }

      // Request account access
      const accounts = await window.ethereum.request({ 
        method: 'eth_requestAccounts' 
      });

      if (accounts.length === 0) {
        throw new Error('No accounts found');
      }

      // Get current network
      const chainId = await window.ethereum.request({ 
        method: 'eth_chainId' 
      });

      // Update state
      this.state = {
        isConnected: true,
        account: accounts[0],
        chainId: chainId,
        networkName: this.getNetworkName(chainId),
      };

      // Notify listeners
      this.onWalletConnected({
        account: this.state.account,
        chainId: this.state.chainId,
        networkName: this.state.networkName,
      });

      console.log('Wallet connected:', this.state);
      return this.state;

    } catch (error) {
      console.error('Failed to connect wallet:', error);
      this.onError(error);
      throw error;
    }
  }

  async disconnect() {
    // Update state
    this.state = {
      isConnected: false,
      account: null,
      chainId: null,
      networkName: null,
    };

    // Notify listeners
    this.onWalletDisconnected();
    
    console.log('Wallet disconnected');
  }

  async checkConnection() {
    try {
      if (typeof window.ethereum === 'undefined') {
        return false;
      }

      const accounts = await window.ethereum.request({ 
        method: 'eth_accounts' 
      });

      if (accounts.length > 0) {
        const chainId = await window.ethereum.request({ 
          method: 'eth_chainId' 
        });

        this.state = {
          isConnected: true,
          account: accounts[0],
          chainId: chainId,
          networkName: this.getNetworkName(chainId),
        };

        return true;
      }

      return false;
    } catch (error) {
      console.error('Failed to check wallet connection:', error);
      return false;
    }
  }

  handleAccountsChanged(accounts) {
    if (accounts.length === 0) {
      // User disconnected
      this.disconnect();
    } else if (accounts[0] !== this.state.account) {
      // Account changed
      this.state.account = accounts[0];
      this.onWalletConnected({
        account: this.state.account,
        chainId: this.state.chainId,
        networkName: this.state.networkName,
      });
    }
  }

  handleChainChanged(chainId) {
    const oldNetworkName = this.state.networkName;
    const newNetworkName = this.getNetworkName(chainId);
    
    this.state.chainId = chainId;
    this.state.networkName = newNetworkName;

    console.log(`Network changed from ${oldNetworkName} to ${newNetworkName}`);
    
    this.onNetworkChanged({
      chainId: chainId,
      networkName: newNetworkName,
      previousNetworkName: oldNetworkName,
    });
  }

  handleDisconnect(error) {
    console.log('Wallet disconnected:', error);
    this.disconnect();
  }

  getNetworkName(chainId) {
    const networks = {
      '0x1': 'mainnet',
      '0x89': 'polygon',
      '0xa4b1': 'arbitrum',
      '0x2105': 'base',
      '0x539': 'testnet', // Local testnet (1337 in decimal)
      '0x7a69': 'testnet', // Another common local testnet (31337 in decimal)
    };

    return networks[chainId] || 'unknown';
  }

  // Map wallet network names to our MCP server network names
  mapToMcpNetworkName(walletNetworkName) {
    const mapping = {
      'mainnet': 'mainnet',
      'polygon': 'polygon',
      'arbitrum': 'arbitrum', 
      'base': 'base',
      'testnet': 'testnet',
      'unknown': 'testnet', // Default to testnet for unknown networks
    };

    return mapping[walletNetworkName] || 'testnet';
  }

  getState() {
    return { ...this.state };
  }

  cleanup() {
    if (typeof window.ethereum !== 'undefined') {
      window.ethereum.removeListener('accountsChanged', this.handleAccountsChanged);
      window.ethereum.removeListener('chainChanged', this.handleChainChanged);
      window.ethereum.removeListener('disconnect', this.handleDisconnect);
    }
  }
}