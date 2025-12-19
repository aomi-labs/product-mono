FROM node:20

WORKDIR /app

# Copy BAML sources into the image so it can serve functions without needing host volumes.
COPY baml_src/ ./baml_src

# Install the BAML CLI. Pin via BAML_CLI_VERSION if provided (defaults to latest).
ARG BAML_CLI_VERSION=latest
RUN if [ "$BAML_CLI_VERSION" = "latest" ]; then \
      npm install -g @boundaryml/baml; \
    else \
      npm install -g @boundaryml/baml@"$BAML_CLI_VERSION"; \
    fi

ENV BAML_SERVER_PORT=2024

# Serve BAML over HTTP.
CMD ["sh", "-c", "baml-cli serve --port ${BAML_SERVER_PORT:-2024}"]
