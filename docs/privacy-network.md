# Privacy and network behavior

Master Time is a local-first utility. It does not include analytics, advertising, accounts, telemetry, cloud synchronization, or a remote application service in the current implementation.

## What leaves the machine

When polling is active, Master Time may:

- perform DNS resolution for the configured NTP hostname;
- send a standard NTP client request over UDP to the resolved address, normally port `123`;
- receive the NTP response from that server.

The request contains the standard NTP client header and no account, machine inventory, application history, or configuration database. The remote NTP operator can still observe normal network metadata such as the source IP address, destination, timing, and packet contents. DNS behavior is provided by the operating system's resolver and may be visible to the configured DNS provider.

## What stays local

The application calculates offset, delay, root distance, health, and rolling history locally. It does not upload measurements or automatically correct the operating system clock. Platform diagnostics are read locally for the presentation layer; they are not sent to a service.

Server names and polling preferences can be represented by the local storage API, but the current desktop shell initializes its built-in configuration in memory and does not automatically load or save a configuration file. See [Configuration persistence](configuration.md).

## Network safeguards

- Each request uses a UDP socket bound to a local ephemeral port.
- The receive operation has a five-second default timeout.
- Responses are checked as NTP packets before being used.
- Server responses must use server mode and provide the timestamps needed for calculation.
- Polling is bounded to a minimum interval of five seconds and a maximum of one hour.
- Stopping the worker requests shutdown; an in-flight bounded request may finish before the worker joins.

These safeguards limit hangs and malformed-input impact, but they do not make an untrusted NTP server trustworthy. Do not use a server you do not intend to contact, and follow your network administrator's policy for UDP/123.

## Choosing a server

The built-in catalog includes public corporate services, distribution services, standards services, and community NTP pools. Public and pool operators have their own availability and acceptable-use policies. A pool hostname may resolve to different addresses over time. Review the selected hostname before starting polling, especially on managed or metered networks.
