"use client";

import { useEffect } from "react";
import { useAppKit } from "@reown/appkit/react";
import { useAppKitAccount, useAppKitNetwork } from "@reown/appkit/react";
import { useEnsName } from "wagmi";
import type { WalletFooterProps } from "@aomi-labs/react";

// Local utility functions
function formatAddress(address: string | undefined): string {
  if (!address) return "";
  return `${address.slice(0, 6)}...${address.slice(-4)}`;
}

function getNetworkName(chainId: number | undefined): string | null {
  if (!chainId) return null;
  const networks: Record<number, string> = {
    1: "Ethereum",
    10: "Optimism",
    137: "Polygon",
    42161: "Arbitrum",
    8453: "Base",
    11155111: "Sepolia",
  };
  return networks[chainId] ?? null;
}

export function WalletFooter({ wallet, setWallet }: Partial<WalletFooterProps>) {
  const { address, isConnected } = useAppKitAccount();
  const { chainId } = useAppKitNetwork();
  const { data: ensName } = useEnsName({
    address: address as `0x${string}` | undefined,
    chainId: 1,
    query: { enabled: Boolean(address) },
  });
  const { open } = useAppKit();

  // Sync AppKit state → widget lib
  useEffect(() => {
    const numericChainId =
      typeof chainId === "string" ? Number(chainId) : chainId;
    setWallet?.({
      address,
      chainId: numericChainId,
      isConnected,
      ensName: ensName ?? undefined,
    });
  }, [address, chainId, isConnected, ensName, setWallet]);

  const networkName = getNetworkName(wallet?.chainId);

  const handleClick = () => {
    if (wallet?.isConnected) {
      void open({ view: "Account" });
    } else {
      void open({ view: "Connect" });
    }
  };

  const label = wallet?.isConnected
    ? (wallet.ensName ?? formatAddress(wallet.address))
    : "Connect Wallet";

  return (
    <div className="p-2">
      <button
        type="button"
        onClick={handleClick}
        className="w-full px-4 py-2 rounded-full bg-gray-900 text-white text-sm font-medium shadow-lg hover:bg-gray-800 transition-colors"
      >
        <div className="flex items-center justify-center gap-2">
          <span>{label}</span>
          {networkName && (
            <span className="text-[11px] text-white/70">• {networkName}</span>
          )}
        </div>
      </button>
    </div>
  );
}
