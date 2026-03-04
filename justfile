# Default recipe: list available recipes
default:
    @just --list

# Build the project
build:
    cargo build

# Run tests
test:
    cargo test

# Install jolene to ~/.cargo/bin
install:
    cargo install --path .
