import type { NextConfig } from "next";

const nextConfig: NextConfig = {
  // Environment variables for different deployment environments
  env: {
    NEXT_PUBLIC_BACKEND_URL: process.env.NEXT_PUBLIC_BACKEND_URL || process.env.BACKEND_URL || 'http://127.0.0.1:8080', // Local dev env
    NEXT_PUBLIC_ANVIL_URL: process.env.NEXT_PUBLIC_ANVIL_URL || process.env.ANVIL_URL || 'http://127.0.0.1:8545',
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
};

export default nextConfig;
