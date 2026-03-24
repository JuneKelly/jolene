FROM rust:slim-bookworm

# System dependencies
RUN apt-get update && apt-get install -y \
    curl \
    git \
    ca-certificates \
    && rm -rf /var/lib/apt/lists/*

# Node.js LTS
RUN curl -fsSL https://deb.nodesource.com/setup_lts.x | bash - \
    && apt-get install -y nodejs \
    && rm -rf /var/lib/apt/lists/*

# just (task runner)
RUN curl -sSL https://just.systems/install.sh | bash -s -- --to /usr/local/bin

# Claude Code
RUN npm install -g @anthropic-ai/claude-code

WORKDIR /workspace
