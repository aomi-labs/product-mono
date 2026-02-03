'use client';

import { useEffect, useState } from 'react';
import { ConnectButton } from '@rainbow-me/rainbowkit';
import { useAccount, useSendTransaction, useWaitForTransactionReceipt, WagmiProvider } from 'wagmi';
import { RainbowKitProvider, getDefaultConfig } from '@rainbow-me/rainbowkit';
import { QueryClient, QueryClientProvider } from '@tanstack/react-query';
import { mainnet, arbitrum, optimism, polygon, base } from 'wagmi/chains';
import { parseEther, formatEther } from 'viem';
import '@rainbow-me/rainbowkit/styles.css';

const config = getDefaultConfig({
  appName: 'Aomi',
  projectId: process.env.NEXT_PUBLIC_WALLETCONNECT_PROJECT_ID || 'demo',
  chains: [mainnet, arbitrum, optimism, polygon, base],
  ssr: false,
});

const queryClient = new QueryClient();

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

function SignContent() {
  const { address, isConnected } = useAccount();
  const [pendingTx, setPendingTx] = useState<PendingTx | null>(null);
  const [status, setStatus] = useState<'loading' | 'ready' | 'signing' | 'success' | 'error' | 'no-tx'>('loading');
  const [error, setError] = useState<string | null>(null);
  const [telegramUserId, setTelegramUserId] = useState<string | null>(null);
  const [txIdParam, setTxIdParam] = useState<string | null>(null);

  const { sendTransaction, data: hash, isPending: isSending, error: sendError } = useSendTransaction();
  const { isLoading: isConfirming, isSuccess: isConfirmed } = useWaitForTransactionReceipt({ hash });

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
    if (!pendingTx || !isConnected) return;

    setStatus('signing');
    
    try {
      sendTransaction({
        to: pendingTx.tx.to as `0x${string}`,
        value: BigInt(pendingTx.tx.value),
        data: pendingTx.tx.data as `0x${string}` | undefined,
      });
    } catch (err) {
      setError(err instanceof Error ? err.message : 'Failed to send transaction');
      setStatus('error');
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
              {hash}
            </p>
            <p className="text-gray-400 text-xs mt-4">Closing...</p>
          </div>
        )}

        {(status === 'ready' || status === 'signing') && pendingTx && (
          <>
            {!isConnected ? (
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
                    disabled={isSending || isConfirming}
                    className="flex-1 py-3 px-4 bg-gray-700 hover:bg-gray-600 rounded-lg font-medium disabled:opacity-50"
                  >
                    Reject
                  </button>
                  <button
                    onClick={handleSign}
                    disabled={isSending || isConfirming}
                    className="flex-1 py-3 px-4 bg-blue-600 hover:bg-blue-500 rounded-lg font-medium disabled:opacity-50"
                  >
                    {isSending ? 'Signing...' : isConfirming ? 'Confirming...' : 'Sign & Send'}
                  </button>
                </div>

                <p className="text-center text-gray-500 text-xs mt-4">
                  Connected: {address?.slice(0, 6)}...{address?.slice(-4)}
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
