default:
    @just --list

clippy:
    cargo clippy

build:
    cargo build

test:
    cargo test

install:
    cargo install --path .

quality:
    @just build 
    @just clippy 
    @just test
