# Installation and running

## Prerequisites

Master Time is a Rust desktop application built with `eframe`/`egui`.

1. Install the current stable Rust toolchain using [rustup](https://rustup.rs/).
2. Confirm that Cargo is available:

   ```text
   rustc --version
   cargo --version
   ```

3. Install the native build dependencies required by `eframe` on your operating system. On Windows, use a supported Visual Studio C++ build environment and Windows SDK. Linux and macOS users should install the system development libraries required by their `eframe` backend.

The repository does not currently provide a platform installer or a prebuilt artifact.

## Run from source

From `master-time`:

```text
cargo run
```

Cargo downloads the Rust dependencies on the first build. To build without launching the application:

```text
cargo build
```

For an optimized binary:

```text
cargo build --release
```

The release binary is written under `target/release/` using the platform's normal executable naming conventions.

## Use the application

1. Launch Master Time.
2. Review the selected NTP server shown in the window.
3. Select **Start polling**. Master Time performs one request immediately, then waits for the configured interval between requests.
4. Review status, server, stratum, offset, round-trip delay, and root distance.
5. Select **Stop polling** before closing if you want an explicit shutdown; the worker also shuts down when dropped.

The desktop window starts at 560×520 pixels and can be resized down to 420×360 pixels.

## Network prerequisites

Allow outbound DNS resolution and UDP traffic to the selected server on port `123`. A response is allowed up to five seconds by default. Firewalls, captive portals, VPNs, restrictive networks, and NTP servers that rate-limit clients can all produce a failed measurement. A failure is shown as an unavailable status; it does not adjust the local clock.

## Troubleshooting

- **Build fails while compiling a native dependency:** install or repair the platform C/C++ toolchain and SDK, then retry `cargo build`.
- **No response from a server:** verify DNS, outbound UDP/123 access, and the server hostname. Try another catalog entry rather than increasing traffic frequency.
- **Invalid polling interval:** supported intervals are from 5 seconds through 1 hour, inclusive.
- **Results disappear after restart:** the reusable storage API exists, but automatic desktop load/save integration is not currently enabled. See [Configuration persistence](configuration.md).
