use anyhow::Result;
use std::collections::VecDeque;
use std::sync::Arc;
use tokio::sync::{Mutex, Notify};
use tracing::{debug, error, info};

/// Request queue for network configuration operations
/// Similar to systemd-networkd's queue mechanism for serializing configuration tasks
#[derive(Clone)]
pub struct RequestQueue {
    inner: Arc<Mutex<RequestQueueInner>>,
    notify: Arc<Notify>,
}

struct RequestQueueInner {
    queue: VecDeque<Request>,
    processing: bool,
}

#[derive(Debug, Clone)]
pub enum Request {
    ConfigureAddress {
        ifindex: u32,
        address: std::net::IpAddr,
        prefix_len: u8,
    },
    ConfigureRoute {
        ifindex: u32,
        destination: Option<std::net::IpAddr>,
        gateway: Option<std::net::IpAddr>,
    },
    SetLinkUp {
        ifindex: u32,
    },
    SetLinkDown {
        ifindex: u32,
    },
    ConfigureDns {
        ifindex: u32,
        servers: Vec<std::net::IpAddr>,
    },
    ActivateConnection {
        profile_id: String,
        device_id: String,
    },
    DeactivateConnection {
        device_id: String,
    },
}

impl RequestQueue {
    pub fn new() -> Self {
        Self {
            inner: Arc::new(Mutex::new(RequestQueueInner {
                queue: VecDeque::new(),
                processing: false,
            })),
            notify: Arc::new(Notify::new()),
        }
    }

    /// Add a request to the queue
    pub async fn enqueue(&self, request: Request) {
        let mut inner = self.inner.lock().await;
        debug!("Enqueuing request: {:?}", request);
        inner.queue.push_back(request);
        self.notify.notify_one();
    }

    /// Add a high-priority request to the front of the queue
    pub async fn enqueue_front(&self, request: Request) {
        let mut inner = self.inner.lock().await;
        debug!("Enqueuing high-priority request: {:?}", request);
        inner.queue.push_front(request);
        self.notify.notify_one();
    }

    /// Get the next request from the queue
    pub async fn dequeue(&self) -> Option<Request> {
        let mut inner = self.inner.lock().await;
        inner.queue.pop_front()
    }

    /// Wait for the next request
    pub async fn wait_for_request(&self) -> Request {
        loop {
            if let Some(request) = self.dequeue().await {
                return request;
            }
            self.notify.notified().await;
        }
    }

    /// Check if the queue is empty
    pub async fn is_empty(&self) -> bool {
        let inner = self.inner.lock().await;
        inner.queue.is_empty()
    }

    /// Get the number of pending requests
    pub async fn len(&self) -> usize {
        let inner = self.inner.lock().await;
        inner.queue.len()
    }

    /// Clear all pending requests
    pub async fn clear(&self) {
        let mut inner = self.inner.lock().await;
        inner.queue.clear();
        info!("Cleared request queue");
    }

    /// Mark queue as processing
    pub async fn set_processing(&self, processing: bool) {
        let mut inner = self.inner.lock().await;
        inner.processing = processing;
    }

    /// Check if queue is currently processing
    pub async fn is_processing(&self) -> bool {
        let inner = self.inner.lock().await;
        inner.processing
    }

    /// Process all requests in the queue with a handler function
    pub async fn process_all<F, Fut>(&self, mut handler: F) -> Result<()>
    where
        F: FnMut(Request) -> Fut,
        Fut: std::future::Future<Output = Result<()>>,
    {
        self.set_processing(true).await;

        loop {
            let request = match self.dequeue().await {
                Some(req) => req,
                None => break,
            };

            debug!("Processing request: {:?}", request);

            if let Err(e) = handler(request.clone()).await {
                error!("Failed to process request {:?}: {}", request, e);
                // Continue processing other requests even if one fails
            }
        }

        self.set_processing(false).await;
        Ok(())
    }
}

impl Default for RequestQueue {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn enqueue_and_dequeue() {
        let queue = RequestQueue::new();

        queue.enqueue(Request::SetLinkUp { ifindex: 2 }).await;
        assert_eq!(queue.len().await, 1);

        let request = queue.dequeue().await;
        assert!(matches!(request, Some(Request::SetLinkUp { ifindex: 2 })));
        assert!(queue.is_empty().await);
    }

    #[tokio::test]
    async fn enqueue_front_priority() {
        let queue = RequestQueue::new();

        queue.enqueue(Request::SetLinkUp { ifindex: 2 }).await;
        queue
            .enqueue_front(Request::SetLinkDown { ifindex: 3 })
            .await;

        let first = queue.dequeue().await;
        assert!(matches!(first, Some(Request::SetLinkDown { ifindex: 3 })));

        let second = queue.dequeue().await;
        assert!(matches!(second, Some(Request::SetLinkUp { ifindex: 2 })));
    }

    #[tokio::test]
    async fn clear_queue() {
        let queue = RequestQueue::new();

        queue.enqueue(Request::SetLinkUp { ifindex: 2 }).await;
        queue.enqueue(Request::SetLinkDown { ifindex: 3 }).await;
        assert_eq!(queue.len().await, 2);

        queue.clear().await;
        assert!(queue.is_empty().await);
    }

    #[tokio::test]
    async fn process_all_requests() {
        let queue = RequestQueue::new();
        let processed = Arc::new(Mutex::new(Vec::new()));

        queue.enqueue(Request::SetLinkUp { ifindex: 2 }).await;
        queue.enqueue(Request::SetLinkDown { ifindex: 3 }).await;

        let processed_clone = processed.clone();
        queue
            .process_all(|request| {
                let processed = processed_clone.clone();
                async move {
                    let mut p = processed.lock().await;
                    p.push(format!("{:?}", request));
                    Ok(())
                }
            })
            .await
            .unwrap();

        let p = processed.lock().await;
        assert_eq!(p.len(), 2);
        assert!(queue.is_empty().await);
    }
}
