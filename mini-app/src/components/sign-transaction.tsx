'use client';

import { useEffect, useState } from 'react';
import { ConnectButton } from '@rainbow-me/rainbowkit';
import { useAccount, useSendTransaction, useWaitForTransactionReceipt, WagmiProvider } from 'wagmi';
import { RainbowKitProvider, getDefaultConfig } from '@rainbow-me/rainbowkit';
import { QueryClient, QueryClientProvider } from '@tanstack/react-query';
import { mainnet, arbitrum, optimism, polygon, base } from 'wagmi/chains';
import { createPublicClient, createWalletClient, formatEther, http, type Chain } from 'viem';
import { privateKeyToAccount } from 'viem/accounts';
import '@rainbow-me/rainbowkit/styles.css';

const config = getDefaultConfig({
  appName: 'Aomi',
  projectId: process.env.NEXT_PUBLIC_WALLETCONNECT_PROJECT_ID || 'demo',
  chains: [mainnet, arbitrum, optimism, polygon, base],
  ssr: false,
});

const queryClient = new QueryClient();
const LOCAL_PRIVATE_KEY_STORAGE_KEY = 'aomi_aa_wallet_private_key_v1';

// Telegram WebApp types are defined in src/types/telegram.d.ts

interface PendingTx {
  txId: string;
  tx: {
    to: string;
    value: string;
    data?: string;
    chainId: number;
  };
  createdAt: number;
}

const CHAIN_NAMES: Record<number, string> = {
  1: 'Ethereum',
  42161: 'Arbitrum',
  10: 'Optimism',
  137: 'Polygon',
  8453: 'Base',
};

const CHAIN_BY_ID: Record<number, Chain> = {
  1: mainnet,
  42161: arbitrum,
  10: optimism,
  137: polygon,
  8453: base,
};

function SignContent() {
  const { address, isConnected } = useAccount();
  const [pendingTx, setPendingTx] = useState<PendingTx | null>(null);
  const [status, setStatus] = useState<'loading' | 'ready' | 'signing' | 'success' | 'error' | 'no-tx'>('loading');
  const [error, setError] = useState<string | null>(null);
  const [telegramUserId, setTelegramUserId] = useState<string | null>(null);
  const [txIdParam, setTxIdParam] = useState<string | null>(null);
  const [localPrivateKey, setLocalPrivateKey] = useState<`0x${string}` | null>(null);
  const [localWalletAddress, setLocalWalletAddress] = useState<string | null>(null);
  const [localHash, setLocalHash] = useState<`0x${string}` | null>(null);
  const [isLocalSending, setIsLocalSending] = useState(false);

  const { sendTransaction, data: hash, isPending: isSending, error: sendError } = useSendTransaction();
  const { isLoading: isConfirming, isSuccess: isConfirmed } = useWaitForTransactionReceipt({ hash });
  const txHash = localHash || hash || null;

  // Initialize Telegram and get tx_id from start_param
  useEffect(() => {
    const tg = window.Telegram?.WebApp;
    if (tg) {
      tg.ready();
      tg.expand();
      
      const user = tg.initDataUnsafe.user;
      if (user) {
        setTelegramUserId(user.id.toString());
      }
      
      // Get tx_id from start_param (passed via Mini App URL)
      const startParam = tg.initDataUnsafe.start_param;
      if (startParam) {
        setTxIdParam(startParam);
      }
    }

    try {
      const storedKey = window.localStorage.getItem(LOCAL_PRIVATE_KEY_STORAGE_KEY);
      if (storedKey) {
        const account = privateKeyToAccount(storedKey as `0x${string}`);
        setLocalPrivateKey(storedKey as `0x${string}`);
        setLocalWalletAddress(account.address);
      }
    } catch {
      // Ignore malformed local key and continue with external wallet flow.
    }
    
    // Also check URL params for web testing
    const urlParams = new URLSearchParams(window.location.search);
    const txId = urlParams.get('tx_id');
    if (txId) {
      setTxIdParam(txId);
    }
  }, []);

  // Fetch pending transaction
  useEffect(() => {
    if (!telegramUserId && !txIdParam) return;

    const fetchTx = async () => {
      try {
        const params = new URLSearchParams();
        if (txIdParam) {
          params.set('tx_id', txIdParam);
        } else if (telegramUserId) {
          params.set('session_key', `telegram:dm:${telegramUserId}`);
        }

        const response = await fetch(`/api/wallet/tx?${params}`);
        const data = await response.json();

        if (data.pending) {
          setPendingTx({
            txId: data.txId,
            tx: data.tx,
            createdAt: data.createdAt,
          });
          setStatus('ready');
        } else {
          setStatus('no-tx');
        }
      } catch (err) {
        setError('Failed to fetch transaction');
        setStatus('error');
      }
    };

    fetchTx();
  }, [telegramUserId, txIdParam]);

  // Handle tx confirmation
  useEffect(() => {
    if (isConfirmed && hash && pendingTx) {
      // Update tx status on backend
      fetch('/api/wallet/tx', {
        method: 'PUT',
        headers: { 'Content-Type': 'application/json' },
        body: JSON.stringify({
          tx_id: pendingTx.txId,
          status: 'signed',
          tx_hash: hash,
        }),
      });

      setStatus('success');
      window.Telegram?.WebApp?.HapticFeedback?.notificationOccurred('success');
      
      setTimeout(() => {
        window.Telegram?.WebApp?.close();
      }, 3000);
    }
  }, [isConfirmed, hash, pendingTx]);

  // Handle send error
  useEffect(() => {
    if (sendError) {
      setError(sendError.message);
      setStatus('error');
      window.Telegram?.WebApp?.HapticFeedback?.notificationOccurred('error');
    }
  }, [sendError]);

  const handleSign = async () => {
    if (!pendingTx) return;

    setStatus('signing');
    setError(null);
    
    try {
      if (localPrivateKey) {
        setIsLocalSending(true);
        const chain = CHAIN_BY_ID[pendingTx.tx.chainId];
        if (!chain) {
          throw new Error(`Unsupported chain ID: ${pendingTx.tx.chainId}`);
        }

        const account = privateKeyToAccount(localPrivateKey);
        const rpcUrl = chain.rpcUrls.default.http[0];
        if (!rpcUrl) {
          throw new Error(`No RPC URL configured for chain ID: ${pendingTx.tx.chainId}`);
        }

        const walletClient = createWalletClient({
          account,
          chain,
          transport: http(rpcUrl),
        });

        const txHash = await walletClient.sendTransaction({
          account,
          to: pendingTx.tx.to as `0x${string}`,
          value: BigInt(pendingTx.tx.value),
          data: pendingTx.tx.data ? (pendingTx.tx.data as `0x${string}`) : undefined,
        });

        const publicClient = createPublicClient({
          chain,
          transport: http(rpcUrl),
        });

        await publicClient.waitForTransactionReceipt({ hash: txHash });

        setLocalHash(txHash);
        await fetch('/api/wallet/tx', {
          method: 'PUT',
          headers: { 'Content-Type': 'application/json' },
          body: JSON.stringify({
            tx_id: pendingTx.txId,
            status: 'signed',
            tx_hash: txHash,
          }),
        });

        setStatus('success');
        window.Telegram?.WebApp?.HapticFeedback?.notificationOccurred('success');

        setTimeout(() => {
          window.Telegram?.WebApp?.close();
        }, 3000);
      } else {
        if (!isConnected) return;
        sendTransaction({
          to: pendingTx.tx.to as `0x${string}`,
          value: BigInt(pendingTx.tx.value),
          data: pendingTx.tx.data as `0x${string}` | undefined,
        });
      }
    } catch (err) {
      setError(err instanceof Error ? err.message : 'Failed to send transaction');
      setStatus('error');
    } finally {
      setIsLocalSending(false);
    }
  };

  const handleReject = async () => {
    if (!pendingTx) return;

    await fetch('/api/wallet/tx', {
      method: 'PUT',
      headers: { 'Content-Type': 'application/json' },
      body: JSON.stringify({
        tx_id: pendingTx.txId,
        status: 'rejected',
      }),
    });

    window.Telegram?.WebApp?.HapticFeedback?.notificationOccurred('warning');
    window.Telegram?.WebApp?.close();
  };

  // Format value for display
  const formatValue = (value: string) => {
    try {
      const wei = BigInt(value);
      return formatEther(wei);
    } catch {
      return value;
    }
  };

  return (
    <main className="min-h-screen bg-gradient-to-b from-gray-900 to-black text-white p-6">
      <div className="max-w-md mx-auto">
        <h1 className="text-2xl font-bold text-center mb-6">🔐 Sign Transaction</h1>

        {status === 'loading' && (
          <div className="text-center">
            <div className="animate-spin rounded-full h-12 w-12 border-t-2 border-b-2 border-white mx-auto mb-4"></div>
            <p className="text-gray-400">Loading transaction...</p>
          </div>
        )}

        {status === 'no-tx' && (
          <div className="bg-gray-800 rounded-lg p-6 text-center">
            <p className="text-gray-400">No pending transaction found.</p>
            <button
              onClick={() => window.Telegram?.WebApp?.close()}
              className="mt-4 px-4 py-2 bg-gray-700 rounded-lg"
            >
              Close
            </button>
          </div>
        )}

        {status === 'error' && (
          <div className="bg-red-900/50 border border-red-500 rounded-lg p-4 mb-6">
            <p className="text-red-400">❌ {error}</p>
            <button
              onClick={() => setStatus('ready')}
              className="text-sm text-gray-400 underline mt-2"
            >
              Try again
            </button>
          </div>
        )}

        {status === 'success' && (
          <div className="bg-green-900/50 border border-green-500 rounded-lg p-6 text-center">
            <p className="text-green-400 text-xl mb-2">✅ Transaction Sent!</p>
            <p className="text-gray-300 text-sm font-mono break-all">
              {txHash}
            </p>
            <p className="text-gray-400 text-xs mt-4">Closing...</p>
          </div>
        )}

        {(status === 'ready' || status === 'signing') && pendingTx && (
          <>
            {!localPrivateKey && !isConnected ? (
              <div className="text-center mb-6">
                <p className="text-gray-400 mb-4">Connect your wallet to sign</p>
                <ConnectButton />
              </div>
            ) : (
              <>
                {/* Transaction Details */}
                <div className="bg-gray-800 rounded-lg p-4 mb-6">
                  <div className="space-y-3">
                    <div>
                      <p className="text-gray-400 text-sm">To</p>
                      <p className="font-mono text-sm break-all">{pendingTx.tx.to}</p>
                    </div>
                    <div>
                      <p className="text-gray-400 text-sm">Value</p>
                      <p className="text-xl font-bold">{formatValue(pendingTx.tx.value)} ETH</p>
                    </div>
                    <div>
                      <p className="text-gray-400 text-sm">Network</p>
                      <p>{CHAIN_NAMES[pendingTx.tx.chainId] || `Chain ${pendingTx.tx.chainId}`}</p>
                    </div>
                    {pendingTx.tx.data && (
                      <div>
                        <p className="text-gray-400 text-sm">Data</p>
                        <p className="font-mono text-xs break-all bg-gray-900 p-2 rounded">
                          {pendingTx.tx.data.slice(0, 66)}...
                        </p>
                      </div>
                    )}
                  </div>
                </div>

                {/* Action Buttons */}
                <div className="flex gap-4">
                  <button
                    onClick={handleReject}
                    disabled={isSending || isConfirming || isLocalSending}
                    className="flex-1 py-3 px-4 bg-gray-700 hover:bg-gray-600 rounded-lg font-medium disabled:opacity-50"
                  >
                    Reject
                  </button>
                  <button
                    onClick={handleSign}
                    disabled={isSending || isConfirming || isLocalSending}
                    className="flex-1 py-3 px-4 bg-blue-600 hover:bg-blue-500 rounded-lg font-medium disabled:opacity-50"
                  >
                    {isLocalSending || isSending ? 'Signing...' : isConfirming ? 'Confirming...' : 'Sign & Send'}
                  </button>
                </div>

                <p className="text-center text-gray-500 text-xs mt-4">
                  {localWalletAddress
                    ? `Local wallet: ${localWalletAddress.slice(0, 6)}...${localWalletAddress.slice(-4)}`
                    : `Connected: ${address?.slice(0, 6)}...${address?.slice(-4)}`}
                </p>
              </>
            )}
          </>
        )}
      </div>
    </main>
  );
}

export default function SignTransaction() {
  return (
    <WagmiProvider config={config}>
      <QueryClientProvider client={queryClient}>
        <RainbowKitProvider>
          <SignContent />
        </RainbowKitProvider>
      </QueryClientProvider>
    </WagmiProvider>
  );
}
