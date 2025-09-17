import type { NextConfig } from "next";

const nextConfig: NextConfig = {
  turbopack: {
    root: "/Users/ceciliazhang/Code/forge-mcp/frontend",
  },
  env: {
    NEXT_PUBLIC_BACKEND_URL: process.env.BACKEND_URL || 'http://localhost:8080',
    NEXT_PUBLIC_ANVIL_URL: process.env.ANVIL_URL || 'http://127.0.0.1:8545',
  },
};

export default nextConfig;
