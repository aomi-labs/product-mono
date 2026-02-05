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

// Resolve @aomi-labs/react - only needed for local widget development
// When using npm packages, let webpack resolve normally
const aomiReactPath = localReactPath || undefined;

// Find the widget src path - either local or from node_modules
const getWidgetSrcPath = (): string | undefined => {
  if (localWidgetSrcPath) return localWidgetSrcPath;
  try {
    const widgetMain = require.resolve("@aomi-labs/widget-lib");
    return dirname(widgetMain);
  } catch {
    return undefined;
  }
};
const widgetSrcPath = getWidgetSrcPath();

// Shared dependencies that must be deduplicated when using local widget
const sharedDeps: Record<string, string> = shouldUseLocalWidget
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

  output: "standalone",

  images: {
    unoptimized: true,
  },

  eslint: {
    ignoreDuringBuilds: true,
  },

  typescript: {
    ignoreBuildErrors: true,
  },

  serverExternalPackages: ["porto"],

  transpilePackages: [
    "@reown/appkit",
    "@reown/appkit-adapter-wagmi",
    // widget-lib exports raw TypeScript, needs transpilation
    "@aomi-labs/widget-lib",
    // react is pre-compiled, only transpile if using local source
    ...(localReactPath ? ["@aomi-labs/react"] : []),
  ],

  turbopack: {
    resolveAlias: {
      porto: emptyModulePath,
      "pino-pretty": emptyModulePath,
      ...(localWidgetPath ? { "@aomi-labs/widget-lib": localWidgetPath } : {}),
      ...(aomiReactPath ? { "@aomi-labs/react": aomiReactPath } : {}),
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
      ...(aomiReactPath ? { "@aomi-labs/react": aomiReactPath } : {}),
      ...sharedDeps,
    };

    // Resolve @/ path aliases in @aomi-labs packages
    if (widgetSrcPath) {
      config.resolve.plugins = config.resolve.plugins ?? [];
      config.resolve.plugins.push({
        apply(resolver: {
          getHook: (name: string) => {
            tapAsync: (
              name: string,
              callback: (
                request: { request?: string; path?: string },
                resolveContext: unknown,
                callback: (err?: Error | null, result?: unknown) => void,
              ) => void,
            ) => void;
          };
          doResolve: (
            hook: unknown,
            request: unknown,
            message: string,
            resolveContext: unknown,
            callback: (err?: Error | null, result?: unknown) => void,
          ) => void;
        }) {
          const target = resolver.getHook("resolve");
          resolver
            .getHook("described-resolve")
            .tapAsync(
              "WidgetAliasPlugin",
              (
                request: { request?: string; path?: string },
                resolveContext: unknown,
                callback: (err?: Error | null, result?: unknown) => void,
              ) => {
                const innerRequest = request.request;
                if (!innerRequest?.startsWith("@/")) {
                  return callback();
                }

                const issuer = request.path ?? "";
                const isFromWidget =
                  issuer.includes("@aomi-labs/widget-lib") ||
                  issuer.includes("@aomi-labs+widget-lib") ||
                  (localWidgetSrcPath && issuer.includes(localWidgetSrcPath));

                if (!isFromWidget) {
                  return callback();
                }

                const newRequest = innerRequest.replace(
                  /^@\//,
                  `${widgetSrcPath}/`,
                );
                resolver.doResolve(
                  target,
                  { ...request, request: newRequest },
                  `Aliased @/ to widget src`,
                  resolveContext,
                  callback,
                );
              },
            );
        },
      });
    }

    return config;
  },
};

export default nextConfig;
