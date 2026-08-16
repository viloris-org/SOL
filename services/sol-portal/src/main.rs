use sol_portal::PortalService;
use sol_system::{
    DefaultDenyPolicy, MemoryActionAuditStore, MemoryPermissionStore, SystemActionService,
};

fn main() {
    let _portal = PortalService::new(SystemActionService::new(
        DefaultDenyPolicy,
        MemoryPermissionStore::default(),
        MemoryActionAuditStore::default(),
    ));
    println!("sol-portal: typed permission-bound request service ready");
}
