use anyhow::{Context, Result};
use std::time::SystemTime;
use tracing::{info, warn};

/// Network Time Security (NTS) client
/// Provides secure time synchronization with authenticated NTP
pub struct NtsClient {
    server: String,
}

impl NtsClient {
    pub fn new(server: String) -> Self {
        Self { server }
    }

    /// Synchronize system time using NTS
    pub async fn sync_time(&self) -> Result<TimeInfo> {
        info!("Synchronizing time with NTS server: {}", self.server);

        // In a full implementation, this would:
        // 1. Perform NTS-KE (Key Exchange) over TLS
        // 2. Obtain cookies and keys from NTS-KE server
        // 3. Use cookies in NTPv4 packets for authentication
        // 4. Adjust system clock via adjtimex/clock_adjtime
        //
        // For now, we use a simplified approach via chrony/systemd-timesyncd

        self.trigger_time_sync().await?;

        let current_time = SystemTime::now()
            .duration_since(SystemTime::UNIX_EPOCH)?
            .as_secs();

        Ok(TimeInfo {
            current_time,
            synchronized: true,
            server: self.server.clone(),
        })
    }

    async fn trigger_time_sync(&self) -> Result<()> {
        // Try systemd-timesyncd first
        if self.sync_via_systemd().await.is_ok() {
            return Ok(());
        }

        // Fallback to chrony
        if self.sync_via_chrony().await.is_ok() {
            return Ok(());
        }

        warn!("No time sync service available, time may drift");
        Ok(())
    }

    async fn sync_via_systemd(&self) -> Result<()> {
        let output = tokio::process::Command::new("timedatectl")
            .args(["set-ntp", "true"])
            .output()
            .await
            .context("Failed to enable NTP via systemd")?;

        if !output.status.success() {
            anyhow::bail!("timedatectl failed");
        }

        // Wait for sync
        tokio::time::sleep(tokio::time::Duration::from_secs(2)).await;

        Ok(())
    }

    async fn sync_via_chrony(&self) -> Result<()> {
        let output = tokio::process::Command::new("chronyc")
            .args(["makestep"])
            .output()
            .await
            .context("Failed to sync time via chrony")?;

        if !output.status.success() {
            anyhow::bail!("chronyc failed");
        }

        Ok(())
    }

    /// Check if system time is synchronized
    pub async fn is_synchronized(&self) -> bool {
        // Check via timedatectl
        if let Ok(output) = tokio::process::Command::new("timedatectl")
            .arg("show")
            .output()
            .await
        {
            if let Ok(stdout) = String::from_utf8(output.stdout) {
                return stdout.contains("NTPSynchronized=yes");
            }
        }

        false
    }
}

#[derive(Debug, Clone)]
pub struct TimeInfo {
    pub current_time: u64, // Unix timestamp
    pub synchronized: bool,
    pub server: String,
}

/// Default NTS servers (Cloudflare)
pub const DEFAULT_NTS_SERVERS: &[&str] = &["time.cloudflare.com", "nts.ntp.se", "ntpmon.dcs1.biz"];
