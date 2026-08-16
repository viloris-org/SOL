use sol_notificationd::{MemoryNotificationStore, NotificationDaemon};

fn main() {
    match NotificationDaemon::new(MemoryNotificationStore::new()) {
        Ok(_) => println!("sol-notificationd: typed notification service ready"),
        Err(error) => eprintln!("sol-notificationd: failed to initialize: {error}"),
    }
}
