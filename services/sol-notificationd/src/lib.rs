//! Notification service core and storage boundary.
//!
//! `sol-notificationd` owns notification IDs, lifecycle transitions, and
//! replacement semantics.  Application and Shell callers depend only on the
//! typed [`sol_system::NotificationApi`] contract.

use sol_system::{
    NotificationActionId, NotificationActionInvocation, NotificationApi, NotificationDismissReason,
    NotificationError, NotificationId, NotificationLifecycle, NotificationQuery,
    NotificationRecord, NotificationRequest, NotificationResult,
};
use std::sync::Mutex;

/// Stored service state. Storage adapters persist this value, not client API
/// implementation details.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NotificationSnapshot {
    /// ID assigned to the next new notification.
    pub next_id: u64,
    /// Sequence assigned to the next record update.
    pub next_sequence: u64,
    /// Retained active and dismissed notification records.
    pub records: Vec<NotificationRecord>,
}

impl Default for NotificationSnapshot {
    fn default() -> Self {
        Self::with_defaults()
    }
}

impl NotificationSnapshot {
    fn with_defaults() -> Self {
        Self {
            next_id: 1,
            next_sequence: 1,
            records: Vec::new(),
        }
    }
}

/// Storage boundary for notification history.
///
/// A file, database, or policy-managed history implementation can replace the
/// in-memory store without changing application or Shell APIs.
pub trait NotificationStore: Send + Sync {
    /// Load the last stored daemon state, if notification history exists.
    fn load(&self) -> NotificationResult<Option<NotificationSnapshot>>;

    /// Persist a complete, internally consistent daemon state.
    fn save(&self, snapshot: &NotificationSnapshot) -> NotificationResult<()>;
}

/// In-memory notification history suitable for tests and embedded development.
#[derive(Debug, Default)]
pub struct MemoryNotificationStore {
    snapshot: Mutex<Option<NotificationSnapshot>>,
}

impl MemoryNotificationStore {
    /// Create an empty memory-backed store.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }
}

impl NotificationStore for MemoryNotificationStore {
    fn load(&self) -> NotificationResult<Option<NotificationSnapshot>> {
        self.snapshot
            .lock()
            .map_err(|error| {
                NotificationError::backend(format!("notification store lock poisoned: {error}"))
            })
            .map(|snapshot| snapshot.clone())
    }

    fn save(&self, snapshot: &NotificationSnapshot) -> NotificationResult<()> {
        let mut stored = self.snapshot.lock().map_err(|error| {
            NotificationError::backend(format!("notification store lock poisoned: {error}"))
        })?;
        *stored = Some(snapshot.clone());
        Ok(())
    }
}

/// Typed notification service with write-through history storage.
#[derive(Debug)]
pub struct NotificationDaemon<S> {
    store: S,
    snapshot: Mutex<NotificationSnapshot>,
}

impl<S: NotificationStore> NotificationDaemon<S> {
    /// Restore notification history from `store` or initialize a new history.
    ///
    /// # Errors
    ///
    /// Returns an error if the backing store cannot be read.
    pub fn new(store: S) -> NotificationResult<Self> {
        let snapshot = store
            .load()?
            .unwrap_or_else(NotificationSnapshot::with_defaults);
        Ok(Self {
            store,
            snapshot: Mutex::new(snapshot),
        })
    }

    /// Return the backing store for service setup and diagnostics.
    #[must_use]
    pub fn store(&self) -> &S {
        &self.store
    }

    fn next_id(snapshot: &mut NotificationSnapshot) -> NotificationResult<NotificationId> {
        let id = NotificationId::from_raw(snapshot.next_id);
        snapshot.next_id = snapshot
            .next_id
            .checked_add(1)
            .ok_or_else(|| NotificationError::backend("notification ID overflow"))?;
        Ok(id)
    }

    fn next_sequence(snapshot: &mut NotificationSnapshot) -> NotificationResult<u64> {
        let sequence = snapshot.next_sequence;
        snapshot.next_sequence = snapshot
            .next_sequence
            .checked_add(1)
            .ok_or_else(|| NotificationError::backend("notification sequence overflow"))?;
        Ok(sequence)
    }

    fn update_and_persist<T>(
        &self,
        update: impl FnOnce(&mut NotificationSnapshot) -> NotificationResult<T>,
    ) -> NotificationResult<T> {
        let mut current = self.snapshot.lock().map_err(|error| {
            NotificationError::backend(format!("notification state lock poisoned: {error}"))
        })?;
        let mut next = current.clone();
        let result = update(&mut next)?;
        self.store.save(&next)?;
        *current = next;
        Ok(result)
    }
}

impl<S: NotificationStore> NotificationApi for NotificationDaemon<S> {
    fn publish(&self, request: NotificationRequest) -> NotificationResult<NotificationRecord> {
        self.update_and_persist(|snapshot| {
            let sequence = Self::next_sequence(snapshot)?;
            if let Some(replaces) = request.replaces {
                let record = snapshot
                    .records
                    .iter_mut()
                    .find(|record| record.id == replaces)
                    .ok_or(NotificationError::NotFound(replaces))?;
                if record.notification.app_id != request.app_id {
                    return Err(NotificationError::ReplacementOwnerMismatch);
                }
                record.notification = request;
                record.lifecycle = NotificationLifecycle::Active;
                record.sequence = sequence;
                return Ok(record.clone());
            }

            let record = NotificationRecord {
                id: Self::next_id(snapshot)?,
                notification: request,
                lifecycle: NotificationLifecycle::Active,
                sequence,
            };
            snapshot.records.push(record.clone());
            Ok(record)
        })
    }

    fn dismiss(
        &self,
        id: NotificationId,
        reason: NotificationDismissReason,
    ) -> NotificationResult<NotificationRecord> {
        self.update_and_persist(|snapshot| {
            let sequence = Self::next_sequence(snapshot)?;
            let record = snapshot
                .records
                .iter_mut()
                .find(|record| record.id == id)
                .ok_or(NotificationError::NotFound(id))?;
            record.lifecycle = NotificationLifecycle::Dismissed(reason);
            record.sequence = sequence;
            Ok(record.clone())
        })
    }

    fn query(&self, query: NotificationQuery) -> NotificationResult<Vec<NotificationRecord>> {
        let snapshot = self.snapshot.lock().map_err(|error| {
            NotificationError::backend(format!("notification state lock poisoned: {error}"))
        })?;
        let mut records: Vec<_> = snapshot
            .records
            .iter()
            .filter(|record| match &query {
                NotificationQuery::Active => record.lifecycle == NotificationLifecycle::Active,
                NotificationQuery::History => true,
                NotificationQuery::ForApp {
                    app_id,
                    include_dismissed,
                } => {
                    record.notification.app_id == *app_id
                        && (*include_dismissed || record.lifecycle == NotificationLifecycle::Active)
                }
            })
            .cloned()
            .collect();
        records.sort_by_key(|record| std::cmp::Reverse(record.sequence));
        Ok(records)
    }

    fn invoke_action(
        &self,
        id: NotificationId,
        action_id: NotificationActionId,
    ) -> NotificationResult<NotificationActionInvocation> {
        let snapshot = self.snapshot.lock().map_err(|error| {
            NotificationError::backend(format!("notification state lock poisoned: {error}"))
        })?;
        let record = snapshot
            .records
            .iter()
            .find(|record| record.id == id)
            .ok_or(NotificationError::NotFound(id))?;
        if record.lifecycle != NotificationLifecycle::Active {
            return Err(NotificationError::NotActive(id));
        }
        if !record
            .notification
            .actions
            .iter()
            .any(|action| action.id == action_id)
        {
            return Err(NotificationError::UnknownAction(action_id));
        }
        Ok(NotificationActionInvocation {
            notification_id: id,
            app_id: record.notification.app_id.clone(),
            action_id,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::{MemoryNotificationStore, NotificationDaemon, NotificationStore};
    use sol_system::{
        AppId, NotificationAction, NotificationActionId, NotificationApi,
        NotificationDismissReason, NotificationError, NotificationLifecycle, NotificationQuery,
        NotificationRequest, NotificationUrgency,
    };

    fn app_id(value: &str) -> AppId {
        AppId::parse(value).expect("test app ID should be valid")
    }

    fn action(id: &str, label: &str) -> NotificationAction {
        NotificationAction::new(
            NotificationActionId::new(id).expect("test action ID should be valid"),
            label,
        )
        .expect("test action should be valid")
    }

    #[test]
    fn service_round_trip_replaces_dismisses_and_queries_notification_history() {
        let daemon = NotificationDaemon::new(MemoryNotificationStore::new())
            .expect("empty notification store should initialize");
        let files = app_id("org.sol.files");
        let original = daemon
            .publish(
                NotificationRequest::new(files.clone(), "Copy complete")
                    .expect("request should be valid")
                    .with_body("report.pdf")
                    .with_urgency(NotificationUrgency::Low)
                    .with_actions(vec![action("open", "Open")])
                    .expect("actions should be unique"),
            )
            .expect("publish should succeed");

        let replaced = daemon
            .publish(
                NotificationRequest::new(files.clone(), "Copy complete")
                    .expect("request should be valid")
                    .with_body("report-final.pdf")
                    .replacing(original.id),
            )
            .expect("owned replacement should succeed");
        assert_eq!(replaced.id, original.id);
        assert!(replaced.sequence > original.sequence);
        assert_eq!(
            replaced.notification.body.as_deref(),
            Some("report-final.pdf")
        );

        let active = daemon
            .query(NotificationQuery::Active)
            .expect("active query should succeed");
        assert_eq!(active, vec![replaced.clone()]);

        let dismissed = daemon
            .dismiss(replaced.id, NotificationDismissReason::User)
            .expect("dismiss should succeed");
        assert_eq!(
            dismissed.lifecycle,
            NotificationLifecycle::Dismissed(NotificationDismissReason::User)
        );
        assert!(
            daemon
                .query(NotificationQuery::Active)
                .expect("active query should succeed")
                .is_empty()
        );
        assert_eq!(
            daemon
                .query(NotificationQuery::ForApp {
                    app_id: files,
                    include_dismissed: true,
                })
                .expect("history query should succeed"),
            vec![dismissed]
        );

        let persisted = daemon
            .store()
            .load()
            .expect("memory store should load")
            .expect("writes should persist through the store");
        assert_eq!(persisted.records.len(), 1);
    }

    #[test]
    fn service_validates_actions_and_replacement_ownership() {
        let daemon = NotificationDaemon::new(MemoryNotificationStore::new())
            .expect("empty notification store should initialize");
        let owner = app_id("org.sol.files");
        let published = daemon
            .publish(
                NotificationRequest::new(owner.clone(), "Mounted")
                    .expect("request should be valid")
                    .with_actions(vec![action("open", "Open")])
                    .expect("actions should be unique"),
            )
            .expect("publish should succeed");

        let invoked = daemon
            .invoke_action(
                published.id,
                NotificationActionId::new("open").expect("action ID should be valid"),
            )
            .expect("active action should invoke");
        assert_eq!(invoked.app_id, owner);

        let replacement = NotificationRequest::new(app_id("org.sol.terminal"), "Wrong owner")
            .expect("request should be valid")
            .replacing(published.id);
        assert_eq!(
            daemon.publish(replacement),
            Err(NotificationError::ReplacementOwnerMismatch)
        );
    }
}
