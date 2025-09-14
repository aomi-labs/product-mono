# Multi-stage Dockerfile for forge-mcp production deployment

# Stage 1: Build Rust application
FROM rust:1.75 as rust-builder

WORKDIR /app
COPY chatbot/ ./chatbot/
COPY config.yaml ./

# Build Rust applications in release mode
WORKDIR /app/chatbot
RUN cargo build --release -p mcp-server -p backend

# Stage 2: Build Frontend
FROM node:20-alpine as frontend-builder

WORKDIR /app
COPY aomi-landing/package*.json ./
RUN npm ci --only=production

COPY aomi-landing/ ./
RUN npm run build

# Stage 3: Final runtime image
FROM ubuntu:22.04

# Install required system dependencies
RUN apt-get update && apt-get install -y \
    ca-certificates \
    curl \
    && rm -rf /var/lib/apt/lists/*

# Create app user
RUN useradd -m -s /bin/bash forge-mcp
WORKDIR /home/forge-mcp

# Copy built applications
COPY --from=rust-builder /app/chatbot/target/release/foameow-mcp-server ./bin/
COPY --from=rust-builder /app/chatbot/target/release/backend ./bin/
COPY --from=frontend-builder /app/dist ./frontend/
COPY --from=frontend-builder /app/node_modules ./frontend/node_modules/
COPY --from=frontend-builder /app/package.json ./frontend/

# Copy configuration files
COPY config.yaml ./
COPY scripts/load-config.sh ./scripts/

# Create production environment template
RUN echo '# Production environment variables' > .env.prod && \
    echo '# Copy this and fill in your actual API keys' >> .env.prod && \
    echo 'ANTHROPIC_API_KEY=your-api-key-here' >> .env.prod && \
    echo 'BRAVE_SEARCH_API_KEY=your-brave-key-here' >> .env.prod && \
    echo 'ETHERSCAN_API_KEY=your-etherscan-key-here' >> .env.prod && \
    echo 'ZEROX_API_KEY=your-0x-key-here' >> .env.prod

# Set ownership
RUN chown -R forge-mcp:forge-mcp /home/forge-mcp

# Switch to app user
USER forge-mcp

# Expose production ports
EXPOSE 5001 8081 3001

# Set production environment
ENV FORGE_ENV=production
ENV RUST_LOG=warn
ENV NODE_ENV=production

# Health check
HEALTHCHECK --interval=30s --timeout=3s --start-period=5s --retries=3 \
    CMD curl -f http://localhost:8081/health || exit 1

# Default command runs production services
CMD ["bash", "-c", "export FORGE_ENV=production && ./bin/foameow-mcp-server & ./bin/backend & cd frontend && npm run preview -- --port 3001 --host 0.0.0.0"]