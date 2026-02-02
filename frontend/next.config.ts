import type { NextConfig } from "next";
import { createRequire } from "module";
import { fileURLToPath } from "url";
import { dirname, resolve } from "path";

const __filename = fileURLToPath(import.meta.url);
const __dirname = dirname(__filename);
const require = createRequire(import.meta.url);
const webpack = require("webpack");

const emptyModulePath = resolve(__dirname, "empty-module.js");
const widgetRoot = process.env.AOMI_WIDGET_ROOT;
const localWidgetPath =
  process.env.AOMI_WIDGET_PATH ||
  (widgetRoot ? resolve(widgetRoot, "apps/registry/src/index.ts") : undefined);
const localReactPath =
  process.env.AOMI_REACT_PATH ||
  (widgetRoot ? resolve(widgetRoot, "packages/react/src/index.ts") : undefined);
const localWidgetSrcPath = widgetRoot
  ? resolve(widgetRoot, "apps/registry/src")
  : undefined;
const shouldUseLocalWidget = Boolean(localWidgetPath || localReactPath);

// Shared dependencies that must be deduplicated when using local widget
// These ensure the widget uses the same wagmi/viem instances as the frontend
// Note: Don't alias react/react-dom as it breaks Next.js 15 (needs React 19 internally)
const sharedDeps = shouldUseLocalWidget
  ? {
      wagmi: resolve(__dirname, "node_modules/wagmi"),
      viem: resolve(__dirname, "node_modules/viem"),
      "@tanstack/react-query": resolve(
        __dirname,
        "node_modules/@tanstack/react-query",
      ),
      zustand: resolve(__dirname, "node_modules/zustand"),
    }
  : {};

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
  // When using local widget source, skip type checking since @/ aliases
  // in the widget can't be resolved by TS (only webpack resolves them)
  typescript: {
    ignoreBuildErrors: shouldUseLocalWidget,
  },

  // Server external packages - don't bundle these on the server
  serverExternalPackages: ["porto"],

  // Transpile packages that need it
  transpilePackages: [
    "@reown/appkit",
    "@reown/appkit-adapter-wagmi",
    ...(shouldUseLocalWidget
      ? ["@aomi-labs/widget-lib", "@aomi-labs/react"]
      : []),
  ],

  // Turbopack configuration
  turbopack: {
    resolveAlias: {
      porto: emptyModulePath,
      "pino-pretty": emptyModulePath,
      ...(localWidgetPath ? { "@aomi-labs/widget-lib": localWidgetPath } : {}),
      ...(localReactPath ? { "@aomi-labs/react": localReactPath } : {}),
      ...sharedDeps,
    },
  },

  webpack: (config) => {
    config.resolve = config.resolve ?? {};
    config.resolve.alias = {
      ...(config.resolve.alias ?? {}),
      "pino-pretty": false,
      porto: emptyModulePath,
      ...(localWidgetPath ? { "@aomi-labs/widget-lib": localWidgetPath } : {}),
      ...(localReactPath ? { "@aomi-labs/react": localReactPath } : {}),
      ...sharedDeps,
    };

    if (localWidgetSrcPath) {
      config.plugins = config.plugins ?? [];
      config.plugins.push(
        new webpack.NormalModuleReplacementPlugin(
          /^@\//,
          (resource: {
            contextInfo?: { issuer?: string };
            request: string;
          }) => {
            const issuer = resource.contextInfo?.issuer ?? "";
            if (!issuer.includes(`${localWidgetSrcPath}/`)) return;
            resource.request = resource.request.replace(
              /^@\//,
              `${localWidgetSrcPath}/`,
            );
          },
        ),
      );
    }

    return config;
  },
};

export default nextConfig;
