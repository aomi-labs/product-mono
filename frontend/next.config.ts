import type { NextConfig } from "next";
import path from "path";

const nextConfig: NextConfig = {
  // Environment variables for different deployment environments
  env: {
    NEXT_PUBLIC_BACKEND_URL:
      process.env.NEXT_PUBLIC_BACKEND_URL
      || process.env.BACKEND_URL
      || 'http://localhost:8080', // Local fallback for dev.sh
    NEXT_PUBLIC_ANVIL_URL:
      process.env.NEXT_PUBLIC_ANVIL_URL
      || process.env.ANVIL_URL
      || 'http://127.0.0.1:8545',
  },

  // Output configuration for deployment
  output: 'standalone', // For Docker deployments

  // Asset optimization
  images: {
    unoptimized: true, // For static exports if needed
  },

  // Turbopack configuration
  turbopack: {
    root: process.cwd(), // Use current working directory
  },

  // ESLint configuration for production builds
  eslint: {
    ignoreDuringBuilds: true, // Temporarily ignore for deployment
  },

  // TypeScript configuration
  typescript: {
    ignoreBuildErrors: false, // Strict TypeScript checking
  },

  webpack: (config) => {
    config.resolve = config.resolve ?? {};
    config.resolve.alias = {
      ...(config.resolve.alias ?? {}),
      "@react-native-async-storage/async-storage": path.resolve(
        __dirname,
        "src/polyfills/async-storage.ts"
      ),
      "pino-pretty": false,
    };
    return config;
  },
};


export default nextConfig;
