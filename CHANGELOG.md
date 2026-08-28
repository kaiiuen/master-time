# Changelog

All notable changes to Master Time are documented here.

## [0.1.0] - Initial release

- Added a desktop utility for comparing local time with an NTP time source.
- Added NTP offset, round-trip delay, root distance, and synchronization-health reporting.
- Added repeated polling with a stoppable background worker and rolling measurement history.
- Added a curated NTP server catalog and platform diagnostics where supported.
- Added reusable protocol, calculation, transport, and configuration components with tests.

This is an early product release. The desktop shell currently initializes its built-in configuration in memory; automatic configuration-file loading and saving are not yet wired into the shell.
