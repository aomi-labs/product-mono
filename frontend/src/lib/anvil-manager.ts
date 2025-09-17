// AnvilManager.ts - Manages Anvil blockchain node monitoring (TypeScript version)
import { AnvilManagerConfig, AnvilManagerEventHandlers, AnvilLog } from './types';

export class AnvilManager {
  private config: AnvilManagerConfig;
  private onStatusChange: (isConnected: boolean) => void;
  private onNewLog: (log: AnvilLog) => void;
  private onError: (error: Error) => void;

  private logs: AnvilLog[] = [];
  private isConnected: boolean = false;
  private intervalId: NodeJS.Timeout | null = null;
  private lastBlockNumber: number = 0;

  constructor(config: Partial<AnvilManagerConfig> = {}, eventHandlers: Partial<AnvilManagerEventHandlers> = {}) {
    this.config = {
      anvilUrl: config.anvilUrl || 'http://localhost:8545',
      checkInterval: config.checkInterval || 2000,
      maxLogEntries: config.maxLogEntries || 100,
      ...config
    };

    // Event handlers
    this.onStatusChange = eventHandlers.onStatusChange || (() => {});
    this.onNewLog = eventHandlers.onNewLog || (() => {});
    this.onError = eventHandlers.onError || (() => {});
  }

  start(): void {
    if (this.intervalId) {
      this.stop();
    }

    this.addLog('system', 'Starting Anvil node monitoring...');

    this.intervalId = setInterval(() => {
      this.checkAnvilStatus();
    }, this.config.checkInterval);

    // Initial check
    this.checkAnvilStatus();
  }

  stop(): void {
    if (this.intervalId) {
      clearInterval(this.intervalId);
      this.intervalId = null;
    }

    this.addLog('system', 'Stopped Anvil node monitoring');
    this.setConnectionStatus(false);
  }

  private async checkAnvilStatus(): Promise<void> {
    try {
      const response = await fetch(this.config.anvilUrl, {
        method: 'POST',
        headers: {
          'Content-Type': 'application/json',
        },
        body: JSON.stringify({
          jsonrpc: '2.0',
          method: 'eth_blockNumber',
          params: [],
          id: 1,
        }),
      });

      if (!response.ok) {
        throw new Error(`HTTP ${response.status}: ${response.statusText}`);
      }

      const data = await response.json();

      if (data.result) {
        const blockNumber = parseInt(data.result, 16);

        if (!this.isConnected) {
          this.setConnectionStatus(true);
          this.addLog('system', `Connected to Anvil node at ${this.config.anvilUrl}`);
        }

        if (blockNumber > this.lastBlockNumber) {
          this.addLog('block', `New block mined: #${blockNumber}`);
          this.lastBlockNumber = blockNumber;

          // Check for transactions in the new block
          await this.checkBlockTransactions(blockNumber);
        }
      }

    } catch (error) {
      if (this.isConnected) {
        this.setConnectionStatus(false);
        this.addLog('error', `Lost connection to Anvil node: ${error instanceof Error ? error.message : String(error)}`);
      }

      this.onError(error instanceof Error ? error : new Error(String(error)));
    }
  }

  private async checkBlockTransactions(blockNumber: number): Promise<void> {
    try {
      const response = await fetch(this.config.anvilUrl, {
        method: 'POST',
        headers: {
          'Content-Type': 'application/json',
        },
        body: JSON.stringify({
          jsonrpc: '2.0',
          method: 'eth_getBlockByNumber',
          params: [`0x${blockNumber.toString(16)}`, true],
          id: 2,
        }),
      });

      const data = await response.json();

      if (data.result && data.result.transactions) {
        const transactions = data.result.transactions;

        if (transactions.length > 0) {
          this.addLog('tx', `Block #${blockNumber} contains ${transactions.length} transaction(s)`);

          transactions.forEach((tx: any, index: number) => {
            this.addLog('tx-detail', `  TX ${index + 1}: ${tx.hash?.slice(0, 10)}... (${tx.from?.slice(0, 10)}... → ${tx.to?.slice(0, 10)}...)`);
          });
        }
      }

    } catch (error) {
      this.addLog('warning', `Failed to fetch block transactions: ${error instanceof Error ? error.message : String(error)}`);
    }
  }

  private setConnectionStatus(connected: boolean): void {
    if (this.isConnected !== connected) {
      this.isConnected = connected;
      this.onStatusChange(connected);
    }
  }

  private addLog(type: AnvilLog['type'], message: string): void {
    const log: AnvilLog = {
      timestamp: new Date().toLocaleTimeString(),
      type,
      message,
    };

    this.logs.push(log);

    // Keep only the most recent logs
    if (this.logs.length > this.config.maxLogEntries) {
      this.logs = this.logs.slice(-this.config.maxLogEntries);
    }

    this.onNewLog(log);
  }

  getLogs(): AnvilLog[] {
    return [...this.logs];
  }

  clearLogs(): void {
    this.logs = [];
    this.addLog('system', 'Logs cleared');
  }

  getConnectionStatus(): boolean {
    return this.isConnected;
  }
}