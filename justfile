# Default recipe: list available recipes
default:
    @just --list

# Run clippy
clippy:
    cargo clippy

# Build the project
build:
    cargo build

# Run tests
test:
    cargo test

# Install jolene to ~/.cargo/bin
install:
    cargo install --path .

