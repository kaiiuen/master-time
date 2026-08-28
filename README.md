# Master Time

Master Time is a focused desktop utility for inspecting local time against an NTP time source. It sends standard NTP requests, calculates clock offset and network delay, evaluates synchronization health, and presents the latest result alongside a rolling history.

The product is designed to be understandable and self-contained: measurements are made locally, the network path is explicit, and the core behavior is split into small Rust components that can be tested without a live time server.

## What it does

- Queries an NTP server over UDP port `123`.
- Calculates offset, round-trip delay, and root distance from the NTP four-timestamp exchange.
- Reports synchronization health using the server stratum, leap indicator, and root distance.
- Runs repeated measurements in a stoppable background worker.
- Keeps up to `120` offset samples in memory for the desktop view.
- Shows platform diagnostics where the operating system supports them.
- Includes a curated server catalog with public services and NTP pool entries.

Master Time observes time; it does not change the operating system clock.

## Quick start

Install a current Rust toolchain, then run the desktop application from this directory:

```text
cargo run
```

The first screen starts with the first built-in server selected. Choose **Start polling** to issue an immediate measurement and continue at the configured interval. See [Installation and running](docs/installation.md) for prerequisites, platform notes, and build commands.

## Development

```text
cargo check
cargo test
cargo fmt -- --check
cargo build --release
```

The test suite is intentionally layered: protocol and calculation tests are deterministic, while transport tests use a local UDP socket instead of an external NTP service. See [Testing](docs/testing.md).

## Documentation

- [Installation and running](docs/installation.md) — prerequisites, commands, and troubleshooting
- [Testing](docs/testing.md) — test layers and validation workflow
- [Component architecture](docs/architecture.md) — boundaries and data flow
- [Privacy and network behavior](docs/privacy-network.md) — exactly what leaves the machine
- [Configuration persistence](docs/configuration.md) — validated settings and storage format
- [Release workflow](docs/release.md) — versioning, verification, and packaging guidance

## Project status

The current desktop shell is an early product surface around a complete set of reusable library components. The configuration storage API is implemented and tested, while the desktop shell currently initializes its built-in configuration in memory at startup; automatic loading and saving of that file are not yet wired into the shell. The documentation calls out this distinction wherever it matters.

## License

No license file is currently included in this repository. Confirm the intended license before distributing binaries.
