import { createConfig, http, cookieStorage, createStorage } from "wagmi";
import { lineaSepolia, linea, mainnet as ethereumChain, polygon, arbitrum, base, optimism, localhost, sepolia } from "wagmi/chains";
import { metaMask } from "wagmi/connectors";

export function getConfig() {
  return createConfig({
    chains: [ethereumChain, polygon, arbitrum, base, optimism, sepolia, localhost, lineaSepolia, linea],
    connectors: [metaMask()],
    ssr: true,
    storage: createStorage({
      storage: cookieStorage,
    }),
    transports: {
      [ethereumChain.id]: http(),
      [polygon.id]: http(),
      [arbitrum.id]: http(),
      [base.id]: http(),
      [optimism.id]: http(),
      [sepolia.id]: http(),
      [localhost.id]: http(),
      [lineaSepolia.id]: http(),
      [linea.id]: http(),
    },
  });
}
