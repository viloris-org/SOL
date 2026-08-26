use std::env;
use std::process::ExitCode;
use std::thread;
use std::time::Duration;

use sol_ntpd::{
    Adjustment, NtpClient, NtpError, StepPolicy, SystemClock, apply_sample, select_sample,
};

const DEFAULT_SERVER: &str = "pool.ntp.org";
const MINIMUM_POLL_INTERVAL_SECONDS: u64 = 16;

#[derive(Debug)]
struct Options {
    servers: Vec<String>,
    timeout: Duration,
    interval: Duration,
    once: bool,
    dry_run: bool,
    panic_threshold: Duration,
}

impl Default for Options {
    fn default() -> Self {
        Self {
            servers: Vec::new(),
            timeout: Duration::from_secs(2),
            interval: Duration::from_secs(1_024),
            once: false,
            dry_run: false,
            panic_threshold: Duration::from_secs(1_000),
        }
    }
}

fn main() -> ExitCode {
    let arguments: Vec<_> = env::args().skip(1).collect();
    if arguments
        .iter()
        .any(|argument| argument == "--help" || argument == "-h")
    {
        println!("{}", usage());
        return ExitCode::SUCCESS;
    }
    let options = match parse_options(arguments) {
        Ok(options) => options,
        Err(error) => {
            eprintln!("sol-ntpd: {error}\n\n{}", usage());
            return ExitCode::from(2);
        }
    };

    let client = NtpClient::new(options.timeout);
    loop {
        if let Err(error) = synchronize_once(&client, &options) {
            eprintln!("sol-ntpd: synchronization failed: {error}");
            if options.once {
                return ExitCode::FAILURE;
            }
        }
        if options.once {
            return ExitCode::SUCCESS;
        }
        thread::sleep(options.interval);
    }
}

fn synchronize_once(client: &NtpClient, options: &Options) -> Result<(), NtpError> {
    let mut samples = Vec::new();
    for server in &options.servers {
        match client.query(server) {
            Ok(sample) => samples.push(sample),
            Err(error) => eprintln!("sol-ntpd: source {server} rejected: {error}"),
        }
    }
    let sample = select_sample(&samples).ok_or(NtpError::NoUsableSample)?;
    println!(
        "sol-ntpd: source={} stratum={} offset={:+.6}s delay={:.6}s distance={:.6}s",
        sample.server,
        sample.stratum,
        sample.offset_seconds,
        sample.delay_seconds,
        sample.root_distance_seconds
    );

    if options.dry_run {
        return Ok(());
    }
    let policy = StepPolicy {
        panic_threshold: options.panic_threshold,
        ..StepPolicy::default()
    };
    match apply_sample(&SystemClock, sample, policy)? {
        Adjustment::AlreadySynchronized => println!("sol-ntpd: clock is synchronized"),
        Adjustment::Stepped => println!("sol-ntpd: realtime clock stepped"),
    }
    Ok(())
}

fn parse_options(arguments: impl IntoIterator<Item = String>) -> Result<Options, String> {
    let mut options = Options::default();
    let mut arguments = arguments.into_iter();
    while let Some(argument) = arguments.next() {
        match argument.as_str() {
            "--server" => options
                .servers
                .push(next_value(&mut arguments, "--server")?),
            "--timeout-ms" => {
                let milliseconds =
                    parse_positive_u64(&next_value(&mut arguments, "--timeout-ms")?, "timeout")?;
                options.timeout = Duration::from_millis(milliseconds);
            }
            "--interval" => {
                let seconds =
                    parse_positive_u64(&next_value(&mut arguments, "--interval")?, "interval")?;
                if seconds < MINIMUM_POLL_INTERVAL_SECONDS {
                    return Err(format!(
                        "poll interval must be at least {MINIMUM_POLL_INTERVAL_SECONDS} seconds"
                    ));
                }
                options.interval = Duration::from_secs(seconds);
            }
            "--panic-threshold" => {
                let seconds = parse_positive_u64(
                    &next_value(&mut arguments, "--panic-threshold")?,
                    "panic threshold",
                )?;
                options.panic_threshold = Duration::from_secs(seconds);
            }
            "--once" => options.once = true,
            "--dry-run" => options.dry_run = true,
            _ => return Err(format!("unknown argument {argument}")),
        }
    }

    if options.servers.is_empty() {
        options.servers = env::var("SOL_NTP_SERVERS")
            .ok()
            .map(|servers| {
                servers
                    .split(',')
                    .map(str::trim)
                    .filter(|server| !server.is_empty())
                    .map(str::to_owned)
                    .collect()
            })
            .filter(|servers: &Vec<String>| !servers.is_empty())
            .unwrap_or_else(|| vec![DEFAULT_SERVER.to_owned()]);
    }
    Ok(options)
}

fn next_value(
    arguments: &mut impl Iterator<Item = String>,
    option: &str,
) -> Result<String, String> {
    arguments
        .next()
        .ok_or_else(|| format!("{option} requires a value"))
}

fn parse_positive_u64(value: &str, name: &str) -> Result<u64, String> {
    let parsed = value
        .parse::<u64>()
        .map_err(|_| format!("{name} must be a positive integer"))?;
    if parsed == 0 {
        Err(format!("{name} must be greater than zero"))
    } else {
        Ok(parsed)
    }
}

const fn usage() -> &'static str {
    "Usage: sol-ntpd [OPTIONS]\n\
     \n\
     Options:\n\
       --server HOST[:PORT]       Add an NTP source (repeatable)\n\
       --timeout-ms MILLISECONDS  Per-address timeout (default: 2000)\n\
       --interval SECONDS         Poll interval, minimum 16 (default: 1024)\n\
       --panic-threshold SECONDS  Reject larger clock steps (default: 1000)\n\
       --once                     Synchronize once and exit\n\
       --dry-run                  Measure without setting CLOCK_REALTIME\n\
       -h, --help                 Show this help\n\
     \n\
     SOL_NTP_SERVERS may contain a comma-separated source list."
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_repeatable_servers_and_dry_run() -> Result<(), String> {
        let options = parse_options(
            [
                "--server",
                "one.example",
                "--server",
                "127.0.0.1:8123",
                "--once",
                "--dry-run",
            ]
            .map(str::to_owned),
        )?;
        assert_eq!(options.servers, ["one.example", "127.0.0.1:8123"]);
        assert!(options.once);
        assert!(options.dry_run);
        Ok(())
    }

    #[test]
    fn refuses_an_abusive_poll_interval() -> Result<(), String> {
        let error = parse_options(["--interval", "1"].map(str::to_owned))
            .err()
            .ok_or_else(|| "short poll interval was accepted".to_owned())?;
        assert!(error.contains("at least 16"));
        Ok(())
    }
}
