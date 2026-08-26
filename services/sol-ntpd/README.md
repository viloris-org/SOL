# SOL NTP service

`sol-ntpd` is SOL's NTPv4 unicast client and privileged clock-sync daemon. It
implements the RFC 5905 four-timestamp calculation, NTP era unfolding,
response-origin binding, Kiss-o'-Death handling, synchronization-distance
bounds, and conservative multi-source selection. RFC 8915 Network Time
Security adds authenticated key establishment over TLS 1.3, one-use cookies,
replay-resistant unique identifiers, and AES-SIV authenticated NTP packets.

## Run

Query an NTS source without changing the clock:

```bash
cargo run -p sol-ntpd -- --nts-server time.cloudflare.com --once --dry-run
```

Synchronize continuously (requires `CAP_SYS_TIME`, normally provided by the
system service):

```bash
sol-ntpd --nts-server time.cloudflare.com
```

`SOL_NTS_SERVERS` and `SOL_NTP_SERVERS` accept comma-separated source lists;
`--nts-server` and `--server` are their command-line counterparts. With no
configuration, the daemon uses `time.cloudflare.com` over NTS. Classic NTP is
still available when explicitly configured, but an NTS failure never falls
back to unauthenticated NTP. The default poll interval is 1024 seconds and
values below 16 seconds are rejected. Corrections over 1000 seconds are
rejected by default, matching RFC 5905's panic-threshold guidance.

## Security and scope

Classic NTP packets are unauthenticated. This implementation rejects malformed,
replayed, mismatched, unsynchronized, excessively distant, and outlying
responses, but only NTS authenticates the server and packets. NTS-KE is limited
to TLS 1.3, validates the server certificate against the bundled Mozilla root
set, requires the `ntske/1` ALPN protocol, and supports the mandatory
AEAD_AES_SIV_CMAC_256 algorithm. Key-establishment retries are exponentially
backed off as required by RFC 8915.

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
