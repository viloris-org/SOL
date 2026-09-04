//! AccessKit-backed AT-SPI bridge for SolUI semantic trees.

use crate::{AccessibilityNode, SemanticId, SemanticRole};
use accesskit::{
    Action, ActionHandler, ActionRequest, ActivationHandler, DeactivationHandler, Node, NodeId,
    Role, Tree, TreeId, TreeUpdate,
};
use accesskit_unix::Adapter;
use std::collections::BTreeMap;
use std::sync::Arc;

/// Action delivered from an AT-SPI client to a SolUI semantic control.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AtspiAction {
    /// Move application focus to the semantic control.
    Focus(SemanticId),
    /// Activate a button or select a tab.
    Activate(SemanticId),
}

/// Live Unix accessibility adapter exposing a SolUI tree over AT-SPI.
pub struct AtspiBridge {
    _adapter: Adapter,
}

impl AtspiBridge {
    /// Register an initial SolUI semantic tree with the Unix accessibility
    /// stack. Actions from assistive technology are returned through `handler`.
    #[must_use]
    pub fn new(
        root: &AccessibilityNode,
        handler: impl Fn(AtspiAction) + Send + Sync + 'static,
    ) -> Self {
        let projection = project_tree(root);
        let mut adapter = Adapter::new(
            InitialTree(projection.update),
            ActionForwarder {
                semantic_ids: Arc::new(projection.semantic_ids),
                handler: Arc::new(handler),
            },
            IgnoreDeactivation,
        );
        adapter.update_window_focus_state(true);
        Self { _adapter: adapter }
    }
}

struct Projection {
    update: TreeUpdate,
    semantic_ids: BTreeMap<NodeId, SemanticId>,
}

fn project_tree(root: &AccessibilityNode) -> Projection {
    let mut nodes = Vec::new();
    let mut semantic_ids = BTreeMap::new();
    let mut next_id = 1_u64;
    let mut focus = None;
    let root_id = project_node(
        root,
        &mut next_id,
        &mut nodes,
        &mut semantic_ids,
        &mut focus,
    );
    let mut tree = Tree::new(root_id);
    tree.toolkit_name = Some("SolUI".to_owned());
    tree.toolkit_version = Some(env!("CARGO_PKG_VERSION").to_owned());
    Projection {
        update: TreeUpdate {
            nodes,
            tree: Some(tree),
            tree_id: TreeId::ROOT,
            focus: focus.unwrap_or(root_id),
        },
        semantic_ids,
    }
}

fn project_node(
    source: &AccessibilityNode,
    next_id: &mut u64,
    nodes: &mut Vec<(NodeId, Node)>,
    semantic_ids: &mut BTreeMap<NodeId, SemanticId>,
    focus: &mut Option<NodeId>,
) -> NodeId {
    let id = NodeId::from(*next_id);
    *next_id += 1;
    semantic_ids.insert(id, source.id.clone());

    let child_ids = source
        .children
        .iter()
        .map(|child| project_node(child, next_id, nodes, semantic_ids, focus))
        .collect::<Vec<_>>();
    let mut node = Node::new(match source.role {
        SemanticRole::Group => Role::Pane,
        SemanticRole::Button => Role::Button,
        SemanticRole::TextField => Role::TextInput,
        SemanticRole::Tab => Role::Tab,
        SemanticRole::Slider => Role::Slider,
    });
    node.set_label(source.label.clone());
    node.set_children(child_ids);
    if let Some(value) = &source.value {
        node.set_value(value.clone());
    }
    if source.state.disabled {
        node.set_disabled();
    }
    if source.state.selected {
        node.set_selected(true);
    }
    if matches!(source.role, SemanticRole::TextField) && !source.state.editable {
        node.set_read_only();
    }
    if !source.state.disabled && !matches!(source.role, SemanticRole::Group) {
        node.add_action(Action::Focus);
    }
    if !source.state.disabled && matches!(source.role, SemanticRole::Button | SemanticRole::Tab) {
        node.add_action(Action::Click);
    }
    if source.state.focused {
        *focus = Some(id);
    }
    nodes.push((id, node));
    id
}

#[derive(Clone)]
struct InitialTree(TreeUpdate);

impl ActivationHandler for InitialTree {
    fn request_initial_tree(&mut self) -> Option<TreeUpdate> {
        Some(self.0.clone())
    }
}

struct ActionForwarder<F> {
    semantic_ids: Arc<BTreeMap<NodeId, SemanticId>>,
    handler: Arc<F>,
}

impl<F> ActionHandler for ActionForwarder<F>
where
    F: Fn(AtspiAction) + Send + Sync + 'static,
{
    fn do_action(&mut self, request: ActionRequest) {
        let Some(id) = self.semantic_ids.get(&request.target_node).cloned() else {
            return;
        };
        match request.action {
            Action::Focus => (self.handler)(AtspiAction::Focus(id)),
            Action::Click => (self.handler)(AtspiAction::Activate(id)),
            _ => {}
        }
    }
}

struct IgnoreDeactivation;

impl DeactivationHandler for IgnoreDeactivation {
    fn deactivate_accessibility(&mut self) {}
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::AccessibilityState;

    #[test]
    fn projection_preserves_roles_state_focus_and_actions() {
        let root = AccessibilityNode {
            id: SemanticId::new("root"),
            role: SemanticRole::Group,
            label: "Fixture".to_owned(),
            value: None,
            state: AccessibilityState::default(),
            children: vec![
                AccessibilityNode {
                    id: SemanticId::new("apply"),
                    role: SemanticRole::Button,
                    label: "Apply".to_owned(),
                    value: None,
                    state: AccessibilityState {
                        focused: true,
                        ..AccessibilityState::default()
                    },
                    children: Vec::new(),
                },
                AccessibilityNode {
                    id: SemanticId::new("name"),
                    role: SemanticRole::TextField,
                    label: "Name".to_owned(),
                    value: Some("SOL".to_owned()),
                    state: AccessibilityState {
                        editable: false,
                        ..AccessibilityState::default()
                    },
                    children: Vec::new(),
                },
            ],
        };

        let projection = project_tree(&root);
        assert_eq!(projection.update.focus, NodeId::from(2));
        assert_eq!(projection.update.nodes.len(), 3);
        let button = &projection.update.nodes[0].1;
        assert_eq!(button.role(), Role::Button);
        assert_eq!(button.label(), Some("Apply"));
        assert!(button.supports_action(Action::Click));
        assert!(button.supports_action(Action::Focus));
        let field = &projection.update.nodes[1].1;
        assert_eq!(field.role(), Role::TextInput);
        assert_eq!(field.value(), Some("SOL"));
        assert!(field.is_read_only());
        let root = &projection.update.nodes[2].1;
        assert_eq!(root.role(), Role::Pane);
        assert_eq!(root.children(), &[NodeId::from(2), NodeId::from(3)]);
    }
}
