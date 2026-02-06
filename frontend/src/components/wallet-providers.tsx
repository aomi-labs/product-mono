"use client";

import { createAppKit, AppKitProvider } from "@reown/appkit/react";
import type { AppKitNetwork } from "@reown/appkit-common";
import { mainnet, arbitrum, optimism, base, polygon, sepolia, linea, lineaSepolia, defineChain } from "@reown/appkit/networks";
import { type ReactNode, useMemo, useState, useEffect } from "react";
import {
  cookieStorage,
  cookieToInitialState,
  createStorage,
  WagmiProvider,
  useAccount,
  useSwitchChain,
  type Config,
  type Storage as WagmiStorage,
} from "wagmi";
import { WagmiAdapter } from "@reown/appkit-adapter-wagmi";
import { QueryClient, QueryClientProvider } from "@tanstack/react-query";

// Enable localhost/Anvil network for E2E testing with `npm run dev:localhost`
const useLocalhost = process.env.NEXT_PUBLIC_USE_LOCALHOST === "true";
const LOCALHOST_CHAIN_ID = 31337;

// Custom localhost network for Anvil (local testing)
// Using defineChain for proper AppKit integration
const localhost = defineChain({
  id: 31337,
  caipNetworkId: 'eip155:31337',
  chainNamespace: 'eip155',
  name: 'Localhost',
  nativeCurrency: {
    decimals: 18,
    name: 'Ether',
    symbol: 'ETH',
  },
  rpcUrls: {
    default: { http: ['http://127.0.0.1:8545'] },
  },
  blockExplorers: {
    default: { name: 'Local', url: 'http://127.0.0.1:8545' },
  },
});

// Get projectId from https://dashboard.reown.com
const projectId = process.env.NEXT_PUBLIC_PROJECT_ID;

if (!projectId) {
  throw new Error("NEXT_PUBLIC_PROJECT_ID is not defined - get one from https://dashboard.reown.com");
}

// Networks supported by the app (localhost included conditionally for testing)
export const networks: [AppKitNetwork, ...AppKitNetwork[]] = useLocalhost
  ? [localhost, mainnet, arbitrum, optimism, base, polygon, sepolia, linea, lineaSepolia]
  : [mainnet, arbitrum, optimism, base, polygon, sepolia, linea, lineaSepolia];

// Widen storage typing to satisfy Wagmi's Storage interface across package versions
const wagmiStorage = createStorage({
  storage: cookieStorage,
}) as WagmiStorage;

// Set up the Wagmi Adapter
export const wagmiAdapter = new WagmiAdapter({
  storage: wagmiStorage,
  ssr: true,
  projectId,
  networks,
});

// AppKit provider configuration
export const appKitProviderConfig = {
  adapters: [wagmiAdapter],
  projectId,
  networks,
  defaultNetwork: useLocalhost ? localhost : mainnet,
  metadata: {
    name: "Aomi Labs",
    description: "AI-powered blockchain operations assistant",
    url: typeof window !== "undefined" ? window.location.origin : "https://aomi.dev",
    icons: ["/assets/images/aomi-logo.svg"],
  },
  features: { analytics: !useLocalhost },
  // Custom chain images for localhost
  ...(useLocalhost && {
    chainImages: {
      31337: 'data:image/svg+xml,<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 24 24" fill="%23627EEA"><circle cx="12" cy="12" r="10"/><text x="12" y="16" text-anchor="middle" fill="white" font-size="10" font-family="Arial">L</text></svg>',
    },
  }),
};

// Initialize AppKit immediately (like the example does)
createAppKit(appKitProviderConfig);

/**
 * Component that auto-switches to localhost network when in localhost mode.
 * Must be rendered inside WagmiProvider.
 */
function LocalhostNetworkEnforcer({ children }: { children: ReactNode }) {
  const { isConnected, chainId, connector } = useAccount();
  const { switchChain } = useSwitchChain();

  useEffect(() => {
    // Only enforce in localhost mode
    if (!useLocalhost) return;
    // Only when connected and on wrong chain
    if (!isConnected || chainId === LOCALHOST_CHAIN_ID) return;

    const switchToLocalhost = async () => {
      console.log(`[LocalhostNetworkEnforcer] Switching from chain ${chainId} to localhost (${LOCALHOST_CHAIN_ID})`);
      
      try {
        // First try to add the chain to the wallet
        const provider = await connector?.getProvider();
        if (provider && typeof provider === 'object' && 'request' in provider) {
          const ethProvider = provider as { request: (args: { method: string; params: unknown[] }) => Promise<unknown> };
          try {
            await ethProvider.request({
              method: 'wallet_addEthereumChain',
              params: [{
                chainId: `0x${LOCALHOST_CHAIN_ID.toString(16)}`,
                chainName: 'Localhost',
                nativeCurrency: { name: 'Ether', symbol: 'ETH', decimals: 18 },
                rpcUrls: ['http://127.0.0.1:8545'],
              }],
            });
          } catch (addError) {
            // Chain might already exist, continue with switch
            console.log('[LocalhostNetworkEnforcer] Chain add result:', addError);
          }
        }
        
        // Then switch to it
        switchChain({ chainId: LOCALHOST_CHAIN_ID });
      } catch (error) {
        console.error('[LocalhostNetworkEnforcer] Failed to switch network:', error);
      }
    };

    switchToLocalhost();
  }, [isConnected, chainId, connector, switchChain]);

  return <>{children}</>;
}

type Props = {
  children: ReactNode;
  cookies?: string | null;
};

export function WalletProviders({ children, cookies }: Props) {
  const [queryClient] = useState(() => new QueryClient());
  const initialState = useMemo(() => {
    if (!cookies) return undefined;

    const decodedCookies = cookies
      .split("; ")
      .map((cookie) => {
        const separatorIndex = cookie.indexOf("=");
        if (separatorIndex === -1) return cookie;

        const name = cookie.slice(0, separatorIndex);
        const value = cookie.slice(separatorIndex + 1);

        try {
          return `${name}=${decodeURIComponent(value)}`;
        } catch {
          return cookie;
        }
      })
      .join("; ");

    try {
      return cookieToInitialState(wagmiAdapter.wagmiConfig as Config, decodedCookies);
    } catch (error) {
      console.error("Failed to parse wagmi cookie, falling back to empty state", error);
      return undefined;
    }
  }, [cookies]);

  return (
    <AppKitProvider {...appKitProviderConfig}>
      <WagmiProvider config={wagmiAdapter.wagmiConfig as Config} initialState={initialState}>
        <QueryClientProvider client={queryClient}>
          <LocalhostNetworkEnforcer>{children}</LocalhostNetworkEnforcer>
        </QueryClientProvider>
      </WagmiProvider>
    </AppKitProvider>
  );
}
