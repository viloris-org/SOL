//! sol-deviced: Device control plane service.
//!
//! Implements ADR-0031: Device control plane and hotplug lifecycle.
//! Provides stable device identity, composite device grouping, convergent
//! lifecycle state, authorization, and safe removal orchestration.

mod adapters;
mod dbus;
mod reconcile;
mod types;

use adapters::{Adapter, FakeAdapter};
use dbus::{Device1Interface, OperationRequest};
use reconcile::Reconciler;
use types::*;

use anyhow::Result;
use std::sync::Arc;
use tokio::sync::RwLock;
use tracing::{error, info};
use zbus::ConnectionBuilder;

/// Core service state.
struct ServiceCore {
    reconciler: Reconciler,
    snapshot: DeviceSnapshot,
    adapters: Vec<Box<dyn Adapter>>,
}

impl ServiceCore {
    fn new() -> Self {
        Self {
            reconciler: Reconciler::new(),
            snapshot: DeviceSnapshot::new(),
            adapters: Vec::new(),
        }
    }

    fn add_adapter(&mut self, adapter: Box<dyn Adapter>) {
        self.adapters.push(adapter);
    }

    async fn start_adapters(&mut self) -> Result<()> {
        for adapter in &mut self.adapters {
            adapter.start().await.map_err(|e| {
                anyhow::anyhow!("Failed to start adapter {}: {}", adapter.name(), e)
            })?;
            info!("Started adapter: {}", adapter.name());
        }
        Ok(())
    }

    async fn reconcile_all(&mut self) -> Result<()> {
        let mut all_devices = Vec::new();
        let mut all_functions = Vec::new();

        for adapter in &self.adapters {
            match adapter.enumerate().await {
                Ok(adapter_snapshot) => {
                    all_devices.extend(adapter_snapshot.devices);
                    all_functions.extend(adapter_snapshot.functions);
                }
                Err(e) => {
                    error!("Failed to enumerate adapter {}: {}", adapter.name(), e);
                }
            }
        }

        self.reconciler
            .reconcile_devices(all_devices, &mut self.snapshot);
        self.reconciler
            .reconcile_functions(all_functions, &mut self.snapshot);

        Ok(())
    }

    async fn handle_operation(&mut self, request: OperationRequest) {
        match request {
            OperationRequest::Authorize {
                device_id,
                scope,
                request_id,
            } => {
                info!(?device_id, ?scope, %request_id, "Authorize request");
                // TODO: Implement authorization logic
            }
            OperationRequest::SetTrust {
                device_id,
                policy,
                request_id,
            } => {
                info!(?device_id, ?policy, %request_id, "SetTrust request");
                if let Some(device) = self.snapshot.devices.get_mut(&device_id) {
                    device.trust_policy = policy;
                    self.snapshot.next_revision();
                }
            }
            OperationRequest::Forget {
                device_id,
                request_id,
            } => {
                info!(?device_id, %request_id, "Forget request");
                // TODO: Implement forget logic (remove from persistence)
            }
            OperationRequest::Eject {
                function_id,
                request_id,
            } => {
                info!(?function_id, %request_id, "Eject request");
                // TODO: Coordinate with storage adapter for safe eject
            }
            OperationRequest::PrepareRemoval {
                device_id,
                request_id,
            } => {
                info!(?device_id, %request_id, "PrepareRemoval request");
                if let Err(e) = self.reconciler.begin_quiesce(device_id, &mut self.snapshot) {
                    error!(?device_id, "Failed to begin quiesce: {}", e);
                }
                // TODO: Coordinate with all relevant adapters
            }
            OperationRequest::CancelOperation { operation_id } => {
                info!(?operation_id, "CancelOperation request");
                // TODO: Implement operation cancellation
            }
        }
    }
}

#[tokio::main]
async fn main() -> Result<()> {
    // Initialize tracing
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| "sol_deviced=info".into()),
        )
        .init();

    info!("Starting sol-deviced");

    // Create service core
    let mut core = ServiceCore::new();

    // Add fake adapters for testing (Phase 1 will add real udev/UDisks2 adapters)
    let fake_adapter = Box::new(FakeAdapter::new("fake"));
    core.add_adapter(fake_adapter);

    // Start adapters
    core.start_adapters().await?;

    // Initial reconciliation
    core.reconcile_all().await?;
    info!(
        "Initial reconciliation complete, revision {}",
        core.snapshot.revision
    );

    // Create operation channel
    let (op_tx, mut op_rx) = tokio::sync::mpsc::unbounded_channel::<OperationRequest>();

    // Create D-Bus interface
    let interface = Device1Interface::new(op_tx);

    // Build D-Bus connection
    let connection = ConnectionBuilder::system()?
        .name("org.sol.Device1")?
        .serve_at("/org/sol/Device1", interface)?
        .build()
        .await?;

    info!("D-Bus service registered at org.sol.Device1");

    // Wrap core in Arc<RwLock> for concurrent access
    let core = Arc::new(RwLock::new(core));

    // Spawn reconciliation loop
    let reconcile_core = core.clone();
    tokio::spawn(async move {
        let mut interval = tokio::time::interval(tokio::time::Duration::from_secs(5));
        loop {
            interval.tick().await;
            let mut core_guard = reconcile_core.write().await;
            if let Err(e) = core_guard.reconcile_all().await {
                error!("Reconciliation failed: {}", e);
            }
        }
    });

    // Main operation handling loop
    while let Some(request) = op_rx.recv().await {
        let mut core_guard = core.write().await;
        core_guard.handle_operation(request).await;
    }

    Ok(())
}
