#![cfg(feature = "atspi")]

use atspi_connection::{AccessibilityConnection, set_session_accessibility};
use atspi_proxies::accessible::ObjectRefExt;
use futures_lite::future::block_on;
use sol_ui::{AtspiBridge, Button, InteractionTree, SemanticControl, TextField};
use std::time::{Duration, Instant};

#[test]
fn live_atspi_bus_exposes_the_solui_semantic_tree() {
    block_on(async {
        set_session_accessibility(true)
            .await
            .expect("enable accessibility in the isolated session");

        let apply = Button::new().with_label("Apply changes");
        let name = TextField::new().with_placeholder("Display name");
        let mut interactions = InteractionTree::new("fixture", "SOL AT-SPI fixture");
        interactions.push(SemanticControl::button("apply", &apply));
        interactions.push(SemanticControl::text_field("name", &name));
        interactions.focus("apply");
        let _bridge = AtspiBridge::new(&interactions.accessibility_tree(), |_| {});

        let connection = AccessibilityConnection::new()
            .await
            .expect("connect to the real AT-SPI bus");
        let root = connection
            .root_accessible_on_registry()
            .await
            .expect("query AT-SPI registry root");
        let deadline = Instant::now() + Duration::from_secs(10);
        loop {
            let mut pending = root.get_children().await.expect("query applications");
            let mut labels = Vec::new();
            for _ in 0..4 {
                let mut next = Vec::new();
                for object in pending {
                    let proxy = object
                        .as_accessible_proxy(connection.connection())
                        .await
                        .expect("query accessible object");
                    labels.push(proxy.name().await.expect("query accessible name"));
                    next.extend(
                        proxy
                            .get_children()
                            .await
                            .expect("query accessible children"),
                    );
                }
                pending = next;
            }
            if labels.iter().any(|label| label == "SOL AT-SPI fixture")
                && labels.iter().any(|label| label == "Apply changes")
                && labels.iter().any(|label| label == "Display name")
            {
                break;
            }
            assert!(
                Instant::now() < deadline,
                "SolUI tree never reached AT-SPI; observed labels: {labels:?}"
            );
            std::thread::sleep(Duration::from_millis(50));
        }
    });
}
