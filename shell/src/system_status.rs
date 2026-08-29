//! Live system status for the top bar.
//!
//! [`crate::topbar`] defines the provider contracts; [`crate::networkmanager`],
//! [`crate::pipewire_audio`], and [`crate::upower`] implement them against real
//! services. This module is what a running desktop session uses to hold them
//! together: it connects what it can, records what it could not, and polls each
//! source no more often than that source is worth polling.
//!
//! Nothing here fabricates a value. A service that is absent reports
//! `Unavailable` and one that fails reports `Error`, and the bar renders both as
//! themselves.

use std::time::{Duration, SystemTime, UNIX_EPOCH};

use crate::{
    networkmanager::NetworkManagerProvider,
    pipewire_audio::PipewireAudioProvider,
    topbar::{
        AudioProvider, ClockProvider, ClockStatus, NetworkProvider, PowerProvider, ProviderState,
        TopBarSnapshot,
    },
    upower::UpowerPowerProvider,
};

/// How often the system services behind the bar are re-read.
///
/// Each poll costs a D-Bus round trip and, for audio, two short-lived
/// processes. Battery, connectivity, and volume do not change fast enough to
/// justify paying that every second — and a desktop that spends its idle time
/// spawning `pactl` is a desktop with a measurably worse battery life than the
/// one it is reporting on.
pub const SYSTEM_POLL_INTERVAL: Duration = Duration::from_secs(5);

/// Marker on a clock the Shell knows is not in the user's own time zone.
///
/// SOL has no timezone source yet: `sol-settingsd` does not own one, and
/// resolving `/etc/localtime` would mean a TZif reader in the Shell. Until that
/// exists the bar shows UTC and says so. A clock that silently shows the wrong
/// hour is worse than one that admits which zone it is in.
const UTC_MARK: &str = "Z";

/// A UTC wall clock derived from the system clock.
#[derive(Debug, Default, Clone, Copy)]
pub struct SystemClockProvider;

impl ClockProvider for SystemClockProvider {
    fn clock(&self) -> ProviderState<ClockStatus> {
        match SystemTime::now().duration_since(UNIX_EPOCH) {
            Ok(since_epoch) => ProviderState::Available {
                value: format_utc(since_epoch.as_secs()),
                stale: false,
            },
            // A system clock before 1970 is not a value to present; it is a
            // machine whose clock has not been set.
            Err(error) => ProviderState::Error(format!("system clock is before the epoch: {error}")),
        }
    }
}

/// Split a Unix timestamp into a UTC date and time.
///
/// Uses the standard civil-from-days algorithm rather than a calendar
/// dependency: it is exact for every date the desktop will ever show and adds
/// nothing to the Shell's dependency graph.
fn format_utc(seconds: u64) -> ClockStatus {
    let days = (seconds / 86_400) as i64;
    let time_of_day = seconds % 86_400;
    let (year, month, day) = civil_from_days(days);
    ClockStatus {
        time: format!(
            "{:02}:{:02}{UTC_MARK}",
            time_of_day / 3_600,
            (time_of_day % 3_600) / 60
        ),
        date: format!("{year:04}-{month:02}-{day:02}"),
    }
}

/// Convert days since 1970-01-01 into a proleptic Gregorian calendar date.
fn civil_from_days(days: i64) -> (i64, u32, u32) {
    // Shift the epoch to 0000-03-01 so leap days land at the end of the cycle.
    let shifted = days + 719_468;
    let era = if shifted >= 0 { shifted } else { shifted - 146_096 } / 146_097;
    let day_of_era = shifted - era * 146_097;
    let year_of_era =
        (day_of_era - day_of_era / 1_460 + day_of_era / 36_524 - day_of_era / 146_096) / 365;
    let year = year_of_era + era * 400;
    let day_of_year = day_of_era - (365 * year_of_era + year_of_era / 4 - year_of_era / 100);
    let shifted_month = (5 * day_of_year + 2) / 153;
    let day = (day_of_year - (153 * shifted_month + 2) / 5 + 1) as u32;
    let month = if shifted_month < 10 {
        shifted_month + 3
    } else {
        shifted_month - 9
    } as u32;
    (if month <= 2 { year + 1 } else { year }, month, day)
}

/// The system sources behind the top bar, with the ones that failed remembered.
pub struct SystemStatus {
    clock: SystemClockProvider,
    network: Result<NetworkManagerProvider, String>,
    audio: PipewireAudioProvider,
    power: Result<UpowerPowerProvider, String>,
    cached: TopBarSnapshot,
    polled_at: Option<std::time::Instant>,
}

impl SystemStatus {
    /// Connect to whatever the host offers, without failing on what it does not.
    ///
    /// A missing NetworkManager or UPower is a normal configuration, not an
    /// error the desktop should refuse to start over.
    #[must_use]
    pub fn connect() -> Self {
        let network = NetworkManagerProvider::connect_system().map_err(|error| error.to_string());
        if let Err(error) = &network {
            tracing::info!(%error, "top bar has no NetworkManager source");
        }
        let power = UpowerPowerProvider::connect_system().map_err(|error| error.to_string());
        if let Err(error) = &power {
            tracing::info!(%error, "top bar has no UPower source");
        }

        Self {
            clock: SystemClockProvider,
            network,
            audio: PipewireAudioProvider::default(),
            power,
            cached: TopBarSnapshot {
                clock: ProviderState::Unavailable,
                workspace: ProviderState::Unavailable,
                network: ProviderState::Unavailable,
                audio: ProviderState::Unavailable,
                power: ProviderState::Unavailable,
                activity: ProviderState::Unavailable,
            },
            polled_at: None,
        }
    }

    /// Current bar state.
    ///
    /// The clock is read every call; the system services are re-read at most
    /// once per [`SYSTEM_POLL_INTERVAL`] and otherwise reused, so a caller may
    /// tick this as often as the clock needs without that cadence reaching
    /// D-Bus.
    pub fn snapshot(&mut self) -> TopBarSnapshot {
        let due = self
            .polled_at
            .is_none_or(|at| at.elapsed() >= SYSTEM_POLL_INTERVAL);
        if due {
            self.cached.network = match &self.network {
                Ok(provider) => provider.network(),
                Err(_) => ProviderState::Unavailable,
            };
            self.cached.power = match &self.power {
                Ok(provider) => provider.power(),
                Err(_) => ProviderState::Unavailable,
            };
            self.cached.audio = self.audio.audio();
            self.polled_at = Some(std::time::Instant::now());
        }

        TopBarSnapshot {
            clock: self.clock.clock(),
            ..self.cached.clone()
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_known_timestamp_formats_as_its_utc_date_and_time() {
        // 2026-08-28T09:41:07Z
        let status = format_utc(1_787_910_067);
        assert_eq!(status.date, "2026-08-28");
        assert_eq!(status.time, "09:41Z");
    }

    #[test]
    fn the_epoch_itself_formats_correctly() {
        let status = format_utc(0);
        assert_eq!(status.date, "1970-01-01");
        assert_eq!(status.time, "00:00Z");
    }

    #[test]
    fn a_leap_day_is_not_off_by_one() {
        // 2024-02-29T23:59:00Z
        let status = format_utc(1_709_251_140);
        assert_eq!(status.date, "2024-02-29");
        assert_eq!(status.time, "23:59Z");
    }

    #[test]
    fn a_century_that_is_not_a_leap_year_is_handled() {
        // 1900-03-01T00:00:00Z is 25 508 days before the epoch, so check the
        // forward direction from a date just after: 2100-03-01T00:00:00Z.
        let status = format_utc(4_107_542_400);
        assert_eq!(status.date, "2100-03-01");
    }

    #[test]
    fn the_clock_reports_a_value_rather_than_an_error_on_a_sane_host() {
        assert!(matches!(
            SystemClockProvider.clock(),
            ProviderState::Available { .. }
        ));
    }

    #[test]
    fn every_minute_of_a_day_round_trips_through_the_formatter() {
        for minute in 0..1_440_u64 {
            let status = format_utc(minute * 60);
            let expected = format!("{:02}:{:02}{UTC_MARK}", minute / 60, minute % 60);
            assert_eq!(status.time, expected);
            assert_eq!(status.date, "1970-01-01");
        }
    }
}
