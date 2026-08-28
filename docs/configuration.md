# Configuration persistence

## Current behavior

The desktop shell currently starts with an in-memory configuration containing the first built-in server and the default 60-second polling interval. Closing and reopening the desktop application therefore does not restore user changes yet.

The library does provide a persistence API in `src/storage.rs`:

- `save_config(path, &config)` / `save(path, &config)` write a configuration.
- `load_config(path)` / `load(path)` read and validate one.

A future desktop integration should choose a platform-appropriate per-user path and call these APIs at startup and after an explicitly applied settings change. The path should not be inferred by users from the repository or `target/` directory.

## Stored format

The file is UTF-8, human-readable, and uses one `key=value` record per line:

```text
# Master Time configuration
version=1
poll_interval_secs=60
active_server=0
server_count=1
server=Google%20Public%20NTP\ttime.google.com
```

The actual server record contains a tab between the encoded display name and hostname. Names use percent encoding for bytes outside ASCII unreserved characters (`A-Z`, `a-z`, `0-9`, `-._~`). This keeps spaces, `=`, and non-ASCII names unambiguous.

The persisted application configuration contains:

- format `version`;
- polling interval in seconds;
- active server index, or `none`;
- server count;
- each validated server name and hostname.

Theme, language, and always-on-top are modeled by `LocalSettings`, but are currently separate from `AppConfig` and are not included in this storage format.

## Validation and failure behavior

Polling intervals must be between 5 seconds and 1 hour inclusive. Server names cannot be empty or contain control characters. Hostnames must be non-empty ASCII DNS names with valid labels. The active server index must refer to an existing server.

Loading rejects unknown or duplicate keys, missing fields, unsupported versions, malformed percent encoding, invalid server records, and mismatched server counts. A failed load returns an error rather than silently accepting partial state.

Saving is atomic: Master Time writes a temporary file beside the destination, flushes it, and replaces the destination. If writing or replacement fails, the existing destination is left in place when the operating system permits the atomic replacement contract.

Configuration files contain server names and hostnames only. They do not contain credentials or measurement history. Protect the chosen file path using normal user-account permissions.
