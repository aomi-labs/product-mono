'use client';

import { useEffect, useState } from 'react';
import { ConnectButton } from '@rainbow-me/rainbowkit';
import { useAccount, useDisconnect, WagmiProvider } from 'wagmi';
import { RainbowKitProvider, getDefaultConfig } from '@rainbow-me/rainbowkit';
import { QueryClient, QueryClientProvider } from '@tanstack/react-query';
import { mainnet, arbitrum, optimism, polygon, base } from 'wagmi/chains';
import '@rainbow-me/rainbowkit/styles.css';

const config = getDefaultConfig({
  appName: 'Aomi',
  projectId: process.env.NEXT_PUBLIC_WALLETCONNECT_PROJECT_ID || 'demo',
  chains: [mainnet, arbitrum, optimism, polygon, base],
  ssr: false,
});

const queryClient = new QueryClient();

function ConnectContent() {
  const { address, isConnected } = useAccount();
  const { disconnect } = useDisconnect();
  const [status, setStatus] = useState<'idle' | 'connecting' | 'success' | 'error'>('idle');
  const [error, setError] = useState<string | null>(null);
  const [telegramUser, setTelegramUser] = useState<{ id: number; name: string } | null>(null);

  useEffect(() => {
    const tg = window.Telegram?.WebApp;
    if (tg) {
      tg.ready();
      tg.expand();

      const user = tg.initDataUnsafe.user;
      if (user) {
        setTelegramUser({
          id: user.id,
          name: user.first_name + (user.last_name ? ' ' + user.last_name : ''),
        });
      }
    }
  }, []);

  useEffect(() => {
    if (isConnected && address && telegramUser && status === 'idle') {
      void bindWallet(address);
    }
  }, [isConnected, address, telegramUser, status]);

  const bindWallet = async (walletAddress: string) => {
    if (!telegramUser) {
      setError('Telegram user not found. Please open from Telegram.');
      setStatus('error');
      return;
    }

    setStatus('connecting');

    try {
      const tg = window.Telegram?.WebApp;

      const response = await fetch('/api/wallet/bind', {
        method: 'POST',
        headers: { 'Content-Type': 'application/json' },
        body: JSON.stringify({
          wallet_address: walletAddress,
          platform: 'telegram',
          platform_user_id: telegramUser.id.toString(),
          init_data: tg?.initData || '',
        }),
      });

      if (!response.ok) {
        const data = await response.json();
        throw new Error(data.error || 'Failed to bind wallet');
      }

      setStatus('success');
      tg?.HapticFeedback?.notificationOccurred('success');

      setTimeout(() => {
        tg?.close();
      }, 2000);
    } catch (err) {
      setError(err instanceof Error ? err.message : 'Unknown error');
      setStatus('error');
      window.Telegram?.WebApp?.HapticFeedback?.notificationOccurred('error');
    }
  };

  const handleDisconnect = () => {
    disconnect();
    setStatus('idle');
    setError(null);
  };

  return (
    <main className="min-h-screen bg-gradient-to-b from-gray-900 to-black text-white p-6">
      <div className="max-w-md mx-auto">
        <div className="text-center mb-8">
          <h1 className="text-3xl font-bold mb-2">🔗 Connect Wallet</h1>
          {telegramUser && <p className="text-gray-400">Hi, {telegramUser.name}!</p>}
        </div>

        {status === 'success' && (
          <div className="bg-green-900/50 border border-green-500 rounded-lg p-4 mb-6 text-center">
            <p className="text-green-400 text-lg">✅ Wallet Connected!</p>
            <p className="text-gray-300 text-sm mt-1 font-mono">
              {address?.slice(0, 6)}...{address?.slice(-4)}
            </p>
            <p className="text-gray-400 text-xs mt-2">Closing...</p>
          </div>
        )}

        {status === 'error' && (
          <div className="bg-red-900/50 border border-red-500 rounded-lg p-4 mb-6">
            <p className="text-red-400">❌ {error}</p>
            <button
              onClick={() => {
                setStatus('idle');
                setError(null);
              }}
              className="text-sm text-gray-400 underline mt-2"
            >
              Try again
            </button>
          </div>
        )}

        {status === 'connecting' && (
          <div className="bg-blue-900/50 border border-blue-500 rounded-lg p-4 mb-6 text-center">
            <p className="text-blue-400">⏳ Connecting wallet...</p>
          </div>
        )}

        {status !== 'success' && (
          <div className="flex flex-col items-center gap-4">
            <ConnectButton />

            {isConnected && (
              <button onClick={handleDisconnect} className="text-sm text-gray-400 underline">
                Disconnect
              </button>
            )}
          </div>
        )}

        {!isConnected && status === 'idle' && (
          <div className="mt-8 text-center text-gray-400 text-sm">
            <p>Connect your wallet to link it with your Telegram account.</p>
            <p className="mt-2">To create a new AA wallet, go back and use the chat button.</p>
          </div>
        )}
      </div>
    </main>
  );
}

export default function ConnectWallet() {
  return (
    <WagmiProvider config={config}>
      <QueryClientProvider client={queryClient}>
        <RainbowKitProvider>
          <ConnectContent />
        </RainbowKitProvider>
      </QueryClientProvider>
    </WagmiProvider>
  );
}
