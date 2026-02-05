"use client";

import { createAppKit, AppKitProvider } from "@reown/appkit/react";
import type { AppKitNetwork } from "@reown/appkit-common";
import { mainnet, arbitrum, optimism, base, polygon, sepolia, linea, lineaSepolia } from "@reown/appkit/networks";
import { type ReactNode, useMemo, useState } from "react";
import {
  cookieStorage,
  cookieToInitialState,
  createStorage,
  WagmiProvider,
  type Config,
  type Storage as WagmiStorage,
} from "wagmi";
import { WagmiAdapter } from "@reown/appkit-adapter-wagmi";
import { QueryClient, QueryClientProvider } from "@tanstack/react-query";

// Custom localhost network for Anvil
const localhost = {
  id: 31337,
  name: "Localhost",
  nativeCurrency: { name: "Ether", symbol: "ETH", decimals: 18 },
  rpcUrls: {
    default: { http: ["http://127.0.0.1:8545"] },
  },
} as const satisfies AppKitNetwork;

// Get projectId from https://dashboard.reown.com
const projectId = process.env.NEXT_PUBLIC_PROJECT_ID;

if (!projectId) {
  throw new Error("NEXT_PUBLIC_PROJECT_ID is not defined - get one from https://dashboard.reown.com");
}

// Networks supported by the app
export const networks: [AppKitNetwork, ...AppKitNetwork[]] = [
  mainnet,
  arbitrum,
  optimism,
  base,
  polygon,
  sepolia,
  linea,
  lineaSepolia,
  localhost,
];

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
  defaultNetwork: mainnet,
  metadata: {
    name: "Aomi Labs",
    description: "AI-powered blockchain operations assistant",
    url: typeof window !== "undefined" ? window.location.origin : "https://aomi.dev",
    icons: ["/assets/images/aomi-logo.svg"],
  },
  features: { analytics: false },
};

// Initialize AppKit immediately (like the example does)
createAppKit(appKitProviderConfig);

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
        <QueryClientProvider client={queryClient}>{children}</QueryClientProvider>
      </WagmiProvider>
    </AppKitProvider>
  );
}
