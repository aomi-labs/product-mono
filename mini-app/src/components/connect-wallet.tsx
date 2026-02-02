'use client';

import { useEffect, useState } from 'react';
import { useSearchParams } from 'next/navigation';
import { ConnectButton } from '@rainbow-me/rainbowkit';
import { useAccount, useDisconnect, WagmiProvider } from 'wagmi';
import { RainbowKitProvider, getDefaultConfig } from '@rainbow-me/rainbowkit';
import { QueryClient, QueryClientProvider } from '@tanstack/react-query';
import { mainnet, arbitrum, optimism, polygon, base } from 'wagmi/chains';
import '@rainbow-me/rainbowkit/styles.css';

// Create config only on client side
const config = getDefaultConfig({
  appName: 'Aomi',
  projectId: process.env.NEXT_PUBLIC_WALLETCONNECT_PROJECT_ID || 'demo',
  chains: [mainnet, arbitrum, optimism, polygon, base],
  ssr: false,
});

const queryClient = new QueryClient();

declare global {
  interface Window {
    Telegram?: {
      WebApp: {
        initData: string;
        initDataUnsafe: {
          user?: {
            id: number;
            first_name: string;
            last_name?: string;
            username?: string;
          };
        };
        ready: () => void;
        close: () => void;
        expand: () => void;
        MainButton: {
          text: string;
          show: () => void;
          hide: () => void;
          onClick: (cb: () => void) => void;
        };
        HapticFeedback: {
          notificationOccurred: (type: 'error' | 'success' | 'warning') => void;
        };
      };
    };
  }
}

type Platform = 'telegram' | 'discord' | 'web';

function ConnectContent() {
  const searchParams = useSearchParams();
  const { address, isConnected } = useAccount();
  const { disconnect } = useDisconnect();
  const [status, setStatus] = useState<'idle' | 'connecting' | 'success' | 'error'>('idle');
  const [error, setError] = useState<string | null>(null);
  const [platform, setPlatform] = useState<Platform>('web');
  const [sessionKey, setSessionKey] = useState<string | null>(null);
  const [userName, setUserName] = useState<string | null>(null);

  // Initialize based on platform
  useEffect(() => {
    // Check for session_key in URL (Discord flow)
    const urlSessionKey = searchParams.get('session_key');
    if (urlSessionKey) {
      setSessionKey(urlSessionKey);
      // Parse platform from session key (e.g., "discord:dm:123456")
      if (urlSessionKey.startsWith('discord:')) {
        setPlatform('discord');
        setUserName('Discord User');
      } else {
        setPlatform('web');
      }
      return;
    }

    // Check for Telegram WebApp
    const tg = window.Telegram?.WebApp;
    if (tg && tg.initDataUnsafe?.user) {
      tg.ready();
      tg.expand();
      
      const user = tg.initDataUnsafe.user;
      setPlatform('telegram');
      setUserName(user.first_name + (user.last_name ? ' ' + user.last_name : ''));
      setSessionKey(`telegram:dm:${user.id}`);
    }
  }, [searchParams]);

  // Handle wallet connection
  useEffect(() => {
    if (isConnected && address && sessionKey && status === 'idle') {
      bindWallet(address);
    }
  }, [isConnected, address, sessionKey, status]);

  const bindWallet = async (walletAddress: string) => {
    if (!sessionKey) {
      setError('No session key found. Please open from Discord or Telegram.');
      setStatus('error');
      return;
    }

    setStatus('connecting');
    
    try {
      const tg = window.Telegram?.WebApp;
      
      // For Discord, we pass the session_key directly
      // For Telegram, we use platform + user_id format
      const body = platform === 'telegram' 
        ? {
            wallet_address: walletAddress,
            platform: 'telegram',
            platform_user_id: sessionKey.split(':')[2],
            init_data: tg?.initData || '',
          }
        : {
            wallet_address: walletAddress,
            platform: 'discord',
            platform_user_id: sessionKey.split(':')[2],
            session_key: sessionKey,
          };

      const response = await fetch('/api/wallet/bind', {
        method: 'POST',
        headers: { 'Content-Type': 'application/json' },
        body: JSON.stringify(body),
      });

      if (!response.ok) {
        const data = await response.json();
        throw new Error(data.error || 'Failed to bind wallet');
      }

      setStatus('success');
      
      if (platform === 'telegram') {
        tg?.HapticFeedback?.notificationOccurred('success');
        setTimeout(() => tg?.close(), 2000);
      }
      
    } catch (err) {
      setError(err instanceof Error ? err.message : 'Unknown error');
      setStatus('error');
      if (platform === 'telegram') {
        window.Telegram?.WebApp?.HapticFeedback?.notificationOccurred('error');
      }
    }
  };

  const handleDisconnect = () => {
    disconnect();
    setStatus('idle');
    setError(null);
  };

  const platformIcon = platform === 'discord' ? '🎮' : platform === 'telegram' ? '📱' : '🌐';
  const platformName = platform.charAt(0).toUpperCase() + platform.slice(1);

  return (
    <main className="min-h-screen bg-gradient-to-b from-gray-900 to-black text-white p-6">
      <div className="max-w-md mx-auto">
        {/* Header */}
        <div className="text-center mb-8">
          <h1 className="text-3xl font-bold mb-2">🔗 Connect Wallet</h1>
          {userName && (
            <p className="text-gray-400">{platformIcon} {userName} ({platformName})</p>
          )}
          {!sessionKey && (
            <p className="text-yellow-400 text-sm mt-2">⚠️ No session found - open from Discord or Telegram</p>
          )}
        </div>

        {/* Status Messages */}
        {status === 'success' && (
          <div className="bg-green-900/50 border border-green-500 rounded-lg p-4 mb-6 text-center">
            <p className="text-green-400 text-lg">✅ Wallet Connected!</p>
            <p className="text-gray-300 text-sm mt-1 font-mono">
              {address?.slice(0, 6)}...{address?.slice(-4)}
            </p>
            {platform === 'telegram' && (
              <p className="text-gray-400 text-xs mt-2">Closing...</p>
            )}
            {platform === 'discord' && (
              <p className="text-gray-400 text-xs mt-2">You can now close this window and use /wallet in Discord</p>
            )}
          </div>
        )}

        {status === 'error' && (
          <div className="bg-red-900/50 border border-red-500 rounded-lg p-4 mb-6">
            <p className="text-red-400">❌ {error}</p>
            <button
              onClick={() => { setStatus('idle'); setError(null); }}
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

        {/* Connect Button */}
        {status !== 'success' && (
          <div className="flex flex-col items-center gap-4">
            <ConnectButton />
            
            {isConnected && (
              <button
                onClick={handleDisconnect}
                className="text-sm text-gray-400 underline"
              >
                Disconnect
              </button>
            )}
          </div>
        )}

        {/* Instructions */}
        {!isConnected && status === 'idle' && sessionKey && (
          <div className="mt-8 text-center text-gray-400 text-sm">
            <p>Connect your wallet to link it with your {platformName} account.</p>
            <p className="mt-2">Supported: MetaMask, WalletConnect, Coinbase, etc.</p>
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
