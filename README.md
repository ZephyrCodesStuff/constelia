# `constelia` - Distributed CTF Attack System

`constelia` is a distributed system for running CTF exploits against multiple targets and submitting flags to a CTFd-compatible API.

## Components

- **Scheduler**: Generates and dispatches jobs to the runner
- **Runner**: Executes Python exploits in Docker containers
- **Submitter**: Deduplicates and submits flags to CTFd
- **Common**: Shared types and utilities

## Prerequisites

- Rust 2021 edition
- Docker
- Python 3.9+ (for exploits)

## Building

```bash
cargo build
```

## Configuration

### Targets

Define your targets in `targets.toml`:

```toml
[[targets]]
id = "web1"
host = "web1.ctf.example.com"
port = 80
service = "http"
tags = ["web", "php"]
```

### Exploits

Place your Python exploits in the `exploits/` directory. Each exploit should have a corresponding TOML metadata file:

```toml
name = "web_exploit.py"
description = "SQL injection exploit for PHP web challenges"
author = "CTF Team"
tags = ["web", "php", "sql"]
timeout_seconds = 30
```

## Usage

### Scheduler

```bash
cargo run --bin scheduler -- --targets targets.toml --exploits exploits/
```

### Runner

```bash
cargo run --bin runner
```

### Submitter

```bash
cargo run --bin submitter -- --api-url https://ctf.example.com --api-token your-token
```

## Development

The project uses:
- `tokio` for async runtime
- `tracing` for structured logging
- `serde` + `toml` for configuration
- `bollard` for Docker integration
- `reqwest` for HTTP clients

## License

MIT 
