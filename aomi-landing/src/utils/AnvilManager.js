// AnvilManager.js - Manages connection to Anvil node on port 8545
export class AnvilManager {
  constructor(options = {}, eventHandlers = {}) {
    this.anvilUrl = options.anvilUrl || 'http://localhost:8545';
    this.checkInterval = options.checkInterval || 2000; // Check every 2 seconds
    this.maxLogEntries = options.maxLogEntries || 100;

    // Event handlers
    this.onStatusChange = eventHandlers.onStatusChange || (() => {});
    this.onNewLog = eventHandlers.onNewLog || (() => {});
    this.onError = eventHandlers.onError || (() => {});

    // State
    this.isConnected = false;
    this.logs = [];
    this.blockNumber = 0;
    this.checkIntervalId = null;
    this.startTime = Date.now();
  }

  async start() {
    console.log('Starting Anvil monitoring...');
    this.checkIntervalId = setInterval(() => {
      this.checkAnvilStatus();
    }, this.checkInterval);

    // Initial check
    await this.checkAnvilStatus();
  }

  stop() {
    if (this.checkIntervalId) {
      clearInterval(this.checkIntervalId);
      this.checkIntervalId = null;
    }
    this.isConnected = false;
    this.onStatusChange(false);
  }

  async checkAnvilStatus() {
    try {
      // Try to get the latest block number from Anvil
      const response = await fetch(this.anvilUrl, {
        method: 'POST',
        headers: {
          'Content-Type': 'application/json',
        },
        body: JSON.stringify({
          id: 1,
          jsonrpc: '2.0',
          method: 'eth_blockNumber',
        }),
      });

      if (!response.ok) {
        throw new Error(`HTTP ${response.status}: ${response.statusText}`);
      }

      const data = await response.json();

      if (data.error) {
        throw new Error(data.error.message || 'RPC Error');
      }

      const newBlockNumber = parseInt(data.result, 16);

      // If we weren't connected before, mark as connected
      if (!this.isConnected) {
        this.isConnected = true;
        this.onStatusChange(true);
        this.addLog('system', '✅ Connected to Anvil node');
        this.addLog('info', `📡 RPC endpoint: ${this.anvilUrl}`);
      }

      // Check if we have a new block
      if (newBlockNumber > this.blockNumber) {
        const oldBlockNumber = this.blockNumber;
        this.blockNumber = newBlockNumber;

        if (oldBlockNumber > 0) { // Don't log the first block
          this.addLog('block', `⛏️  New block #${this.blockNumber} mined`);
        } else {
          this.addLog('info', `📊 Current block: #${this.blockNumber}`);
        }

        // Get more detailed block info
        await this.getBlockDetails(newBlockNumber);
      }

    } catch (error) {
      // If we were connected before, mark as disconnected
      if (this.isConnected) {
        this.isConnected = false;
        this.onStatusChange(false);
        this.addLog('error', `❌ Lost connection to Anvil: ${error.message}`);
      } else {
        // Only log connection attempts every 10 seconds to avoid spam
        const elapsed = Date.now() - this.startTime;
        if (elapsed % 10000 < this.checkInterval) {
          this.addLog('warning', `🔍 Trying to connect to Anvil at ${this.anvilUrl}...`);
        }
      }
      this.onError(error);
    }
  }

  async getBlockDetails(blockNumber) {
    try {
      const response = await fetch(this.anvilUrl, {
        method: 'POST',
        headers: {
          'Content-Type': 'application/json',
        },
        body: JSON.stringify({
          id: 1,
          jsonrpc: '2.0',
          method: 'eth_getBlockByNumber',
          params: [`0x${blockNumber.toString(16)}`, true],
        }),
      });

      const data = await response.json();
      if (!data.error && data.result) {
        const block = data.result;
        const txCount = block.transactions ? block.transactions.length : 0;

        if (txCount > 0) {
          this.addLog('tx', `📦 Block #${blockNumber}: ${txCount} transaction${txCount !== 1 ? 's' : ''}`);

          // Log transaction details
          block.transactions.forEach((tx, index) => {
            if (typeof tx === 'object') {
              const gasUsed = tx.gas ? parseInt(tx.gas, 16).toLocaleString() : 'unknown';
              this.addLog('tx-detail', `   └─ TX ${index + 1}: ${tx.hash?.substring(0, 10)}... (gas: ${gasUsed})`);
            }
          });
        }
      }
    } catch (error) {
      console.warn('Failed to get block details:', error);
    }
  }

  addLog(type, message) {
    const timestamp = new Date().toLocaleTimeString();
    const logEntry = {
      timestamp,
      type, // 'system', 'info', 'block', 'tx', 'tx-detail', 'error', 'warning'
      message,
      id: Date.now() + Math.random(), // Simple unique ID
    };

    this.logs.push(logEntry);

    // Keep only the last N log entries
    if (this.logs.length > this.maxLogEntries) {
      this.logs = this.logs.slice(-this.maxLogEntries);
    }

    this.onNewLog(logEntry);
  }

  getLogs() {
    return [...this.logs];
  }

  getStatus() {
    return {
      isConnected: this.isConnected,
      blockNumber: this.blockNumber,
      logCount: this.logs.length,
      anvilUrl: this.anvilUrl,
    };
  }

  clearLogs() {
    this.logs = [];
  }
}