# syntax=docker/dockerfile:1.6

###############################################
# Rust builder – compiles backend + MCP server
# (edition 2024 crates require nightly cargo at the moment)
###############################################
FROM rustlang/rust:nightly-slim AS rust-builder

WORKDIR /workspace

RUN apt-get update \
    && apt-get install -y --no-install-recommends \
        pkg-config \
        libssl-dev \
        clang \
        make \
    && rm -rf /var/lib/apt/lists/*

COPY chatbot ./chatbot

WORKDIR /workspace/chatbot
RUN cargo build --locked --release -p backend -p aomi-mcp

###############################################
# Frontend builder – produces Next.js bundle
###############################################
FROM node:20-bullseye-slim AS frontend-builder

WORKDIR /frontend

RUN apt-get update \
    && apt-get install -y --no-install-recommends \
        python3 \
        python-is-python3 \
        make \
        g++ \
    && rm -rf /var/lib/apt/lists/*

# Dev build defualt to localhos
# Prod build gets AOMI_DOMAIN=aomi.dev from docker-compose.yml
ARG AOMI_DOMAIN=localhost

ENV NEXT_PUBLIC_BACKEND_URL=http://${AOMI_DOMAIN}:8081
ENV NEXT_PUBLIC_ANVIL_URL=http://${AOMI_DOMAIN}:8545

COPY frontend/package*.json ./
RUN npm ci

COPY frontend/ ./
RUN npm run build

###############################################
# Backend runtime image
###############################################
FROM debian:sid-slim AS backend-runtime

RUN apt-get update \
    && apt-get install -y --no-install-recommends \
        ca-certificates \
        curl \
    && rm -rf /var/lib/apt/lists/*

WORKDIR /app

COPY --from=rust-builder /workspace/chatbot/target/release/backend /usr/local/bin/backend
COPY chatbot/documents ./documents
COPY config.yaml ./config.yaml
COPY docker/backend-entrypoint.sh /entrypoint.sh

RUN chmod +x /entrypoint.sh

ENV BACKEND_HOST=0.0.0.0 \
    BACKEND_PORT=8081 \
    BACKEND_SKIP_DOCS=false \
    RUST_LOG=info

EXPOSE 8081

ENTRYPOINT ["/entrypoint.sh"]

###############################################
# MCP runtime image
###############################################
FROM debian:sid-slim AS mcp-runtime

RUN apt-get update \
    && apt-get install -y --no-install-recommends \
        ca-certificates \
        curl \
        python3-minimal \
        python3-yaml \
    && rm -rf /var/lib/apt/lists/*

WORKDIR /app

COPY --from=rust-builder /workspace/chatbot/target/release/aomi-mcp-server /usr/local/bin/aomi-mcp-server
COPY docker/mcp-entrypoint.sh /entrypoint.sh
COPY scripts2/configure.py /app/scripts2/configure.py
COPY config.yaml /app/config.yaml

RUN chmod +x /entrypoint.sh

ENV MCP_SERVER_HOST=0.0.0.0 \
    MCP_SERVER_PORT=5001 \
    MCP_CONFIG_PATH=/app/config.yaml \
    RUST_LOG=info

EXPOSE 5001

ENTRYPOINT ["/entrypoint.sh"]

###############################################
# Frontend runtime image
###############################################
FROM node:20-bullseye-slim AS frontend-runtime

WORKDIR /app

ENV NODE_ENV=production \
    NEXT_TELEMETRY_DISABLED=1 \
    PORT=3000

COPY --from=frontend-builder /frontend/.next/standalone ./
COPY --from=frontend-builder /frontend/.next/static ./.next/static
COPY --from=frontend-builder /frontend/public ./public
COPY --from=frontend-builder /frontend/package.json ./package.json

EXPOSE 3000

CMD ["node", "server.js"]
