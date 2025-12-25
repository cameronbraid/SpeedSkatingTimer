# Speed Skating Timer

A precision timing system for speed skating, featuring hardware integration with IR beam sensors, real-time stopwatch functionality, and a modern web interface.

## Overview

This project provides a complete timing solution for speed skating events:

- **Trigger-driven authoritative timing** — The server is the single source of truth for all timing data
- **Real-time updates** — Live lap times and state changes streamed at up to 10Hz
- **Hardware integration** — GPIO-based IR beam sensors for automatic lap detection
- **Hardware setup mode** — Visual tool for aligning IR diode/sensor pairs
- **Network-first architecture** — NATS messaging enables distributed components and low-latency communication
- **Client-side predictive rendering** — Frontend displays smooth time updates with intentional lag to ensure displayed time never exceeds server-confirmed time

## Architecture

```
┌─────────────────┐      ┌─────────────────┐      ┌─────────────────┐
│   IR Sensors    │────▶│  Rust Backend   │────▶│  React Frontend │
│  (GPIO/RPi)     │      │  (NATS Server)  │◀────│   (WebSocket)   │
└─────────────────┘      └─────────────────┘      └─────────────────┘
                               │
                               ▼
                        ┌─────────────┐
                        │ NATS Server │
                        │ (JetStream) │
                        └─────────────┘
```

The system uses a **trigger abstraction** — the stopwatch service doesn't know about GPIO directly. It consumes logical trigger events, allowing different trigger sources (GPIO, keyboard, test harness) to be used interchangeably.

## Backend

The backend is written in **Rust** and provides:

### Services

| Service | Description |
|---------|-------------|
| **Stopwatch** | Core timing FSM with states: Disarmed → Armed → Running → Stopped |
| **Hardware Setup** | Live GPIO pin state monitoring for IR sensor alignment (~10Hz) |
| **Auth** | PIN-based authentication with role-based NATS permissions |
| **System Ping** | Heartbeat service (2Hz) for client latency measurement |

### Trigger Sources

- **GPIO (Redundant)** — Production mode with multiple IR sensors for redundancy
- **Keyboard** — Development mode for manual trigger input
- **Mock** — Testing mode with no trigger source
- **Simulator** — Automated test sequences for demos

### Key Dependencies

- `tokio` — Async runtime
- `async-nats` — NATS client
- `axum` — HTTP server (serves frontend, WebSocket proxy)
- `gpio-cdev` — Linux GPIO character device interface
- `nats-jwt` — JWT generation for authenticated users

### Running the Backend

```bash

# Development mode with simulator
./backend-dev.sh

# With GPIO triggers (Raspberry Pi)
./backend-gpio.sh

# Or directly with cargo
cd backend
cargo run --bin server -- --dev --simulator
```

### Configuration

- `auth.yaml` — Admin PINs and NATS permission mappings
- `gpio.yaml` — GPIO pin configuration for IR sensors

## Frontend

The frontend is a **React** single-page application built with:

### Technology Stack

- **React 19** with TypeScript
- **Vite** for fast development and optimized builds
- **Mantine** UI component library
- **TanStack Router** for client-side routing
- **NATS WebSocket** for real-time communication

### Pages

| Route | Description |
|-------|-------------|
| `/` | Main stopwatch display with digital 7-segment time, lap times, and controls |
| `/hardware-setup` | IR sensor alignment tool with live pin state indicators |

### Key Features

- **7-segment digital display** — Classic stopwatch aesthetic
- **Live lap tracking** — Real-time lap time display as skaters cross sensors
- **Dynamic lag adjustment** — Automatically compensates for network latency
- **Authenticated controls** — Admin PIN required for arm/disarm/reset

### Running the Frontend

```bash
# Development mode with hot reload
./frontend-dev.sh

# Or directly with pnpm
cd frontend
pnpm dev

# Build for production
./frontend-build.sh
```

## NATS Messaging

The system uses NATS for all communication between components.
### Running NATS

```
./nats.sh
```

### Subject Layout

| Subject | Type | Description |
|---------|------|-------------|
| `stopwatch.v1.arm` | Request/Reply | Arm the stopwatch |
| `stopwatch.v1.unarm` | Request/Reply | Unarm/stop the stopwatch |
| `stopwatch.v1.reset` | Request/Reply | Reset to armed state |
| `stopwatch.v1.trigger` | Pub/Sub | Logical trigger pulses |
| `stopwatch.v1.live.state` | Pub/Sub | State change broadcasts |
| `stopwatch.v1.live.tick` | Pub/Sub | Periodic time updates (5-10Hz) |
| `stopwatch.v1.live.lap` | Pub/Sub | Lap start/finish events |
| `system.v1.heartbeat` | Pub/Sub | Server time heartbeat (2Hz) |
| `hardware-setup.v1.*` | Mixed | Hardware setup service |

### Authentication

The system uses NATS JWT-based authentication:

1. **Frontend user** — Limited permissions, can view data and request auth upgrades
2. **Admin user** — Full permissions after PIN authentication
3. **Server user** — Full access for backend services

## Quick Start

### Prerequisites

- [Rust](https://rustup.rs/) (latest stable)
- [Node.js](https://nodejs.org/) + [pnpm](https://pnpm.io/)
- [NATS Server](https://nats.io/) + [nsc](https://github.com/nats-io/nsc)

### Setup

```bash
# 1. Start NATS server (generates credentials on first run)
./nats.sh

# 2. In a new terminal, start the backend
./backend-dev.sh

# 3. In another terminal, start the frontend
./frontend-dev.sh

# 4. Open http://localhost:8080 in your browser
```

### Production Deployment (Raspberry Pi)

The backend can be deployed as a systemd service on Raspberry Pi:

```bash
cd backend

# Cross-compile for ARM
cross build --release --target=armv7-unknown-linux-gnueabihf

# Deploy application and install/start services
./deploy.sh
```

## Project Structure

```
SpeedSkatingTimer/
├── backend/               # Rust backend
│   ├── src/
│   │   ├── bin/
│   │   │   ├── server.rs  # Main server binary
│   │   │   └── tui.rs     # Terminal UI (development)
│   │   ├── trigger/       # Trigger source implementations
│   │   ├── auth.rs        # Authentication service
│   │   ├── stopwatch.rs   # Core timing logic
│   │   └── ...
│   └── Cargo.toml
├── frontend/              # React frontend
│   ├── src/
│   │   ├── components/    # React components
│   │   ├── hooks/         # Custom React hooks
│   │   ├── routes/        # TanStack Router pages
│   │   └── types/         # TypeScript type definitions
│   └── package.json
├── plans/                 # Architecture documentation
├── nats.sh                # NATS server startup script
├── backend-dev.sh         # Backend development script
├── frontend-dev.sh        # Frontend development script
└── README.md
```

## License

ISC
