#!/usr/bin/env pwsh

# Ensure the script runs from the directory containing Cargo.toml
Set-Location -Path $PSScriptRoot\..

# Enable strict error handling
$ErrorActionPreference = "Stop"

# Set environment variable
$env:RUST_BACKTRACE = 1

Write-Host "Formatting..."
cargo fmt

Write-Host "Running clippy for binary..."
cargo clippy --quiet --bin versatiles --all-features $args

Write-Host "Running clippy for library..."
cargo clippy --quiet --lib --no-default-features $args

Write-Host "Running clippy for library (big)..."
cargo clippy --quiet --lib --all-features $args

Write-Host "Running tests for binary..."
cargo test --quiet --bins --all-features $args

Write-Host "Running tests for library..."
cargo test --quiet --lib --no-default-features $args

Write-Host "Running tests for library (big)..."
cargo test --quiet --lib --all-features $args

Write-Host "Running doc tests (big)..."
cargo test --quiet --doc --all-features $args
