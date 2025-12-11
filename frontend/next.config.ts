import type { NextConfig } from "next";
import { fileURLToPath } from "url";
import { dirname, resolve } from "path";

const __filename = fileURLToPath(import.meta.url);
const __dirname = dirname(__filename);

const emptyModulePath = resolve(__dirname, "empty-module.js");

const nextConfig: NextConfig = {
  // Environment variables for different deployment environments
  env: {
    NEXT_PUBLIC_BACKEND_URL:
      process.env.NEXT_PUBLIC_BACKEND_URL ||
      process.env.BACKEND_URL ||
      "http://localhost:8080",
    NEXT_PUBLIC_ANVIL_URL:
      process.env.NEXT_PUBLIC_ANVIL_URL ||
      process.env.ANVIL_URL ||
      "http://127.0.0.1:8545",
  },

  // Output configuration for deployment
  output: "standalone",

  // Asset optimization
  images: {
    unoptimized: true,
  },

  // ESLint configuration for production builds
  eslint: {
    ignoreDuringBuilds: true,
  },

  // TypeScript configuration
  typescript: {
    ignoreBuildErrors: false,
  },

  // Server external packages - don't bundle these on the server
  serverExternalPackages: ["porto"],

  // Transpile packages that need it
  transpilePackages: ["@reown/appkit", "@reown/appkit-adapter-wagmi"],

  // Turbopack configuration
  turbopack: {
    resolveAlias: {
      porto: emptyModulePath,
      "pino-pretty": emptyModulePath,
    },
  },

  webpack: (config) => {
    config.resolve = config.resolve ?? {};
    config.resolve.alias = {
      ...(config.resolve.alias ?? {}),
      "pino-pretty": false,
      porto: emptyModulePath,
    };

    return config;
  },
};

export default nextConfig;
