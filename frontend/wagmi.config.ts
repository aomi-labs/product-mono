import { createConfig, http, cookieStorage, createStorage } from "wagmi";
import { lineaSepolia, linea, mainnet, polygon, arbitrum, base, localhost } from "wagmi/chains";
import { metaMask } from "wagmi/connectors";

export function getConfig() {
  return createConfig({
    chains: [mainnet, polygon, arbitrum, base, localhost, lineaSepolia, linea],
    connectors: [metaMask()],
    ssr: true,
    storage: createStorage({
      storage: cookieStorage,
    }),
    transports: {
      [mainnet.id]: http(),
      [polygon.id]: http(),
      [arbitrum.id]: http(),
      [base.id]: http(),
      [localhost.id]: http(),
      [lineaSepolia.id]: http(),
      [linea.id]: http(),
    },
  });
}