# SOL NTP service

`sol-ntpd` is SOL's minimal NTPv4 unicast client and privileged clock-sync
daemon. It implements the 48-byte RFC 5905 header, the four-timestamp offset
and round-trip-delay calculation, NTP era unfolding, response-origin binding,
Kiss-o'-Death handling, synchronization-distance bounds, and conservative
multi-source selection.

## Run

Query without changing the clock:

```bash
cargo run -p sol-ntpd -- --server pool.ntp.org --once --dry-run
```

Synchronize continuously (requires `CAP_SYS_TIME`, normally provided by the
system service):

```bash
sol-ntpd --server 0.pool.ntp.org --server 1.pool.ntp.org
```

`SOL_NTP_SERVERS` accepts a comma-separated source list. Command-line
`--server` arguments take precedence. The default poll interval is 1024
seconds and values below 16 seconds are rejected. Corrections over 1000
seconds are rejected by default, matching RFC 5905's panic-threshold guidance.

## Security and scope

Classic NTP packets are unauthenticated. This implementation rejects
malformed, replayed, mismatched, unsynchronized, excessively distant, and
outlying responses, but those checks do not authenticate a server or stop an
on-path delay attack. NTS (RFC 8915) is a required follow-up before this client
is used across an untrusted network for a release security claim.

The current clock discipline is deliberately small: it steps
`CLOCK_REALTIME` after a bounded correction and does not estimate oscillator
frequency or slew small corrections. That is a functional bootstrap client,
not a claim of parity with chrony or the complete RFC 5905 reference clock
discipline.

## Test

```bash
cargo test -p sol-ntpd
cargo clippy -p sol-ntpd --all-targets -- -D warnings
```
