'use client';

import { useEffect, useState } from 'react';
import { generatePrivateKey, privateKeyToAccount } from 'viem/accounts';

const STORAGE_KEY = 'aomi_aa_wallet_private_key_v1';

type Status = 'idle' | 'creating' | 'binding' | 'success' | 'error';

function getTelegramUser() {
  const tg = window.Telegram?.WebApp;
  const user = tg?.initDataUnsafe?.user;
  if (!user) return null;
  return {
    id: user.id,
    name: user.first_name + (user.last_name ? ` ${user.last_name}` : ''),
  };
}

export default function CreateWallet() {
  const [status, setStatus] = useState<Status>('idle');
  const [error, setError] = useState<string | null>(null);
  const [telegramUser, setTelegramUser] = useState<{ id: number; name: string } | null>(null);
  const [walletAddress, setWalletAddress] = useState<string | null>(null);
  const [privateKey, setPrivateKey] = useState<string | null>(null);
  const [hasExistingWallet, setHasExistingWallet] = useState(false);

  useEffect(() => {
    const tg = window.Telegram?.WebApp;
    if (tg) {
      tg.ready();
      tg.expand();
    }

    const user = getTelegramUser();
    setTelegramUser(user);

    try {
      const storedKey = window.localStorage.getItem(STORAGE_KEY);
      if (storedKey) {
        const account = privateKeyToAccount(storedKey as `0x${string}`);
        setPrivateKey(storedKey);
        setWalletAddress(account.address);
        setHasExistingWallet(true);
        void bindWallet(account.address, user?.id);
      }
    } catch (err) {
      setError('Failed to load local wallet from this device');
      setStatus('error');
    }
  }, []);

  const bindWallet = async (address: string, userId = telegramUser?.id) => {
    if (!userId) {
      setError('Telegram user not found. Please open from Telegram.');
      setStatus('error');
      return;
    }

    setStatus('binding');
    try {
      const tg = window.Telegram?.WebApp;
      const response = await fetch('/api/wallet/bind', {
        method: 'POST',
        headers: { 'Content-Type': 'application/json' },
        body: JSON.stringify({
          wallet_address: address,
          platform: 'telegram',
          platform_user_id: userId.toString(),
          init_data: tg?.initData || '',
        }),
      });

      if (!response.ok) {
        const data = await response.json();
        throw new Error(data.error || 'Failed to bind wallet');
      }

      setStatus('success');
      tg?.HapticFeedback?.notificationOccurred('success');
    } catch (err) {
      setError(err instanceof Error ? err.message : 'Failed to bind wallet');
      setStatus('error');
    }
  };

  const handleCreateWallet = async () => {
    setError(null);
    setStatus('creating');

    try {
      const generatedKey = generatePrivateKey();
      const account = privateKeyToAccount(generatedKey);

      window.localStorage.setItem(STORAGE_KEY, generatedKey);

      setPrivateKey(generatedKey);
      setWalletAddress(account.address);
      setHasExistingWallet(false);

      await bindWallet(account.address);
    } catch (err) {
      setError(err instanceof Error ? err.message : 'Failed to create wallet');
      setStatus('error');
    }
  };

  const handleCopy = async () => {
    if (!privateKey) return;
    try {
      await navigator.clipboard.writeText(privateKey);
      window.Telegram?.WebApp?.HapticFeedback?.notificationOccurred('success');
    } catch {
      setError('Failed to copy key');
      setStatus('error');
    }
  };

  return (
    <main className="min-h-screen bg-gradient-to-b from-gray-900 to-black text-white p-6">
      <div className="max-w-md mx-auto">
        <h1 className="text-3xl font-bold mb-2 text-center">🆕 Create AA Wallet</h1>
        {telegramUser && <p className="text-gray-400 text-center mb-6">Hi, {telegramUser.name}!</p>}

        {error && (
          <div className="bg-red-900/40 border border-red-500 rounded-lg p-4 mb-4">
            <p className="text-red-300">❌ {error}</p>
          </div>
        )}

        {!privateKey && (
          <div className="bg-gray-800 rounded-lg p-4 mb-4">
            <p className="text-gray-300 mb-4">
              Create a new wallet on this device. The private key stays local and is never sent to our server.
            </p>
            <button
              onClick={handleCreateWallet}
              disabled={status === 'creating' || status === 'binding'}
              className="w-full py-3 px-4 bg-blue-600 hover:bg-blue-500 rounded-lg font-medium disabled:opacity-50"
            >
              {status === 'creating' ? 'Creating...' : status === 'binding' ? 'Binding...' : 'Create Wallet'}
            </button>
          </div>
        )}

        {privateKey && walletAddress && (
          <div className="bg-gray-800 rounded-lg p-4 space-y-4">
            <p className="text-green-300">
              {hasExistingWallet ? '✅ Existing local wallet loaded' : '✅ Wallet created on this device'}
            </p>

            <div>
              <p className="text-gray-400 text-sm">Address</p>
              <p className="font-mono text-sm break-all">{walletAddress}</p>
            </div>

            <div>
              <p className="text-yellow-300 text-sm font-semibold">Private Key (backup now)</p>
              <p className="font-mono text-xs break-all bg-gray-900 p-2 rounded">{privateKey}</p>
            </div>

            <p className="text-red-300 text-sm">
              ⚠️ We cannot recover this key. If you lose this device and backup, funds are lost.
            </p>

            <div className="flex gap-3">
              <button
                onClick={handleCopy}
                className="flex-1 py-2 px-3 bg-gray-700 hover:bg-gray-600 rounded-lg text-sm"
              >
                Copy Key
              </button>
              <button
                onClick={() => window.Telegram?.WebApp?.close()}
                className="flex-1 py-2 px-3 bg-blue-600 hover:bg-blue-500 rounded-lg text-sm"
              >
                Close
              </button>
            </div>
          </div>
        )}
      </div>
    </main>
  );
}
