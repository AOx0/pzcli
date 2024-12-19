# Project Zomboid CLI

A command-line tool that enables running Project Zomboid (Build 41) dedicated servers in headless mode while maintaining remote interaction capabilities through TCP connections.

## Why?

Project Zomboid's dedicated server can only be interacted with when running in an attached mode. This means you can't run the server in headless mode if you need to:
- Monitor server status
- Execute server commands
- Manage players
- Configure server settings

This tool solves this limitation by:
1. Spawning and managing the Project Zomboid process in headless mode
2. Providing a TCP interface for remote interaction
3. Handling stdout/stderr streams with structured logging

This is particularly useful for dedicated servers running on cloud/remote machines where you want to run the server without a GUI session but still maintain full control.

## Installation

Currently, the tool needs to be built from source:

```bash
cargo build --release
```

## Usage

The tool has two main modes: server and client.

### Starting a Server

```bash
# Start the server on default address (::) and port (9988)
pzcli server start

# Start with custom address and port
# The address and port are for the pzcli server,
# the game still uses default ports for Project Zomboid
pzcli server start 127.0.0.1 8888
```

### Connecting to a Server

```bash
# Connect to local server with default settings
pzcli cli

# Connect to remote server
pzcli cli 192.168.1.100

# Connect to remote server with also custom port
pzcli cli 192.168.1.100 8888
```

### Interactive Commands

Once connected, you can interact with the server using standard Project Zomboid server commands. Some common commands:

- `players` - List connected players
- `quit` or `q` - Stop the server
- `help` - Show available commands

Type `exit` to disconnect from the server without stopping it.

## Requirements

- Project Zomboid Dedicated Server installed
- Rust toolchain for building
- Linux environment (currently hardcoded paths assume Linux)

## Configuration

The tool currently expects the Project Zomboid Dedicated Server to be installed at:
```
/home/ae/.steam/steam/steamapps/common/Project Zomboid Dedicated Server
```

You'll need to modify this path in the source code if your installation is different.
