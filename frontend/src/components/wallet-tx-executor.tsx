"use client";

import { useEffect, useCallback } from "react";
import { useSendTransaction, useAccount } from "wagmi";
import {
  useWalletHandler,
  useAomiRuntime,
  type WalletTxRequest,
} from "@aomi-labs/react";

/**
 * Handles wallet transaction requests from the AI backend.
 * Must be rendered inside both WagmiProvider and AomiRuntimeProvider.
 */
export function WalletTxExecutor() {
  const { isConnected } = useAccount();
  const { currentThreadId } = useAomiRuntime();
  const { pendingTxRequests, sendTxComplete, clearTxRequest } =
    useWalletHandler({ sessionId: currentThreadId });

  const { sendTransactionAsync, isPending } = useSendTransaction();

  const executeTransaction = useCallback(
    async (request: WalletTxRequest, index: number) => {
      if (!isConnected) {
        console.warn("[WalletTxExecutor] Wallet not connected");
        sendTxComplete({ txHash: "", status: "failed" });
        clearTxRequest(index);
        return;
      }

      try {
        console.log("[WalletTxExecutor] Sending transaction:", request);

        const txHash = await sendTransactionAsync({
          to: request.to as `0x${string}`,
          value: request.value ? BigInt(request.value) : undefined,
          data: request.data as `0x${string}` | undefined,
          chainId: request.chainId,
        });

        console.log("[WalletTxExecutor] Transaction sent:", txHash);
        sendTxComplete({ txHash, status: "success", amount: request.value });
      } catch (error) {
        console.error("[WalletTxExecutor] Transaction failed:", error);
        sendTxComplete({ txHash: "", status: "failed" });
      } finally {
        clearTxRequest(index);
      }
    },
    [isConnected, sendTransactionAsync, sendTxComplete, clearTxRequest],
  );

  // Process pending transactions one at a time
  useEffect(() => {
    if (pendingTxRequests.length === 0 || isPending) return;

    const firstRequest = pendingTxRequests[0];
    if (firstRequest) {
      void executeTransaction(firstRequest, 0);
    }
  }, [pendingTxRequests, isPending, executeTransaction]);

  return null;
}
