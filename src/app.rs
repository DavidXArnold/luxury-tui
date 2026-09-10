use std::collections::HashMap;

use ratatui::widgets::ListState;

use crate::config::Config;
use crate::k8s::context::{KubeContext, KubeconfigsByPath};
use crate::k8s::diff::DiffLine;
use crate::k8s::objectmap::MapNode;
use crate::k8s::resources::{EventSummary, NodeSummary, PodSummary, WorkloadSummary};
use crate::theme::Theme;
use kube::Client;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Screen {
    ContextPicker,
    Overview,
    Workloads,
    Attention,
    Logs,
    ObjectMap,
    Compare,
    Help,
}

pub const MAIN_SCREENS: [Screen; 6] = [
    Screen::Overview,
    Screen::Workloads,
    Screen::Attention,
    Screen::Logs,
    Screen::ObjectMap,
    Screen::Compare,
];

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WorkloadKind {
    Pods,
    Deployments,
    StatefulSets,
    DaemonSets,
    ReplicaSets,
    Jobs,
    CronJobs,
    Services,
    Nodes,
}

pub const WORKLOAD_KINDS: [WorkloadKind; 9] = [
    WorkloadKind::Pods,
    WorkloadKind::Deployments,
    WorkloadKind::StatefulSets,
    WorkloadKind::DaemonSets,
    WorkloadKind::ReplicaSets,
    WorkloadKind::Jobs,
    WorkloadKind::CronJobs,
    WorkloadKind::Services,
    WorkloadKind::Nodes,
];

impl WorkloadKind {
    pub fn label(&self) -> &'static str {
        match self {
            WorkloadKind::Pods => "Pods",
            WorkloadKind::Deployments => "Deployments",
            WorkloadKind::StatefulSets => "StatefulSets",
            WorkloadKind::DaemonSets => "DaemonSets",
            WorkloadKind::ReplicaSets => "ReplicaSets",
            WorkloadKind::Jobs => "Jobs",
            WorkloadKind::CronJobs => "CronJobs",
            WorkloadKind::Services => "Services",
            WorkloadKind::Nodes => "Nodes",
        }
    }

    /// Matches `WorkloadSummary::kind` as produced by the `k8s::resources`
    /// listers (singular), used as the cache key.
    pub fn api_kind(&self) -> &'static str {
        match self {
            WorkloadKind::Pods => "Pod",
            WorkloadKind::Deployments => "Deployment",
            WorkloadKind::StatefulSets => "StatefulSet",
            WorkloadKind::DaemonSets => "DaemonSet",
            WorkloadKind::ReplicaSets => "ReplicaSet",
            WorkloadKind::Jobs => "Job",
            WorkloadKind::CronJobs => "CronJob",
            WorkloadKind::Services => "Service",
            WorkloadKind::Nodes => "Node",
        }
    }
}

pub struct LogViewerState {
    pub lines: Vec<String>,
    pub search: String,
    /// While true, keystrokes on the Logs screen type into `search` instead
    /// of being treated as commands (so e.g. an 'f' while searching doesn't
    /// also toggle follow).
    pub editing_search: bool,
    pub follow: bool,
    pub target: Option<(String, String, Option<String>)>, // namespace, pod, container
    pub scroll: usize,
    /// Every stream is requested with Kubernetes timestamps on; this only
    /// controls whether we strip them back off before display.
    pub show_timestamps: bool,
    /// Render lines that parse as JSON as `key=value` pairs instead of raw
    /// JSON — easier to scan for structured logs.
    pub json_pretty: bool,
}

impl Default for LogViewerState {
    fn default() -> Self {
        LogViewerState {
            lines: vec![],
            search: String::new(),
            editing_search: false,
            follow: false,
            target: None,
            scroll: 0,
            show_timestamps: true,
            json_pretty: false,
        }
    }
}

/// Identifies one resource by kind/namespace/name — used both to remember
/// the current Workloads-screen selection across refreshes and to name a
/// diff target (see `k8s::diff::compare_resources`). `kind` matches
/// `WorkloadKind::api_kind()`; `namespace` is empty for cluster-scoped
/// kinds (Node).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ItemRef {
    pub kind: String,
    pub namespace: String,
    pub name: String,
}

#[derive(Default)]
pub struct CompareState {
    pub left: Option<ItemRef>,
    pub right: Option<ItemRef>,
    pub diff: Vec<DiffLine>,
}

#[derive(Default)]
pub struct PaletteState {
    pub active: bool,
    pub query: String,
    pub selected: usize,
}

#[derive(Default)]
pub struct ObjectMapState {
    pub root: Option<MapNode>,
    pub rendered: String,
}

pub struct App {
    pub config: Config,
    pub contexts: Vec<KubeContext>,
    /// The parsed kubeconfig each discovered context came from, keyed by
    /// its resolved file path — looked up when a context is picked, since
    /// building a client needs the *specific* file, not just the name.
    pub kubeconfig_files: KubeconfigsByPath,
    pub context_selected: usize,
    pub client: Option<Client>,
    pub current_context_name: Option<String>,
    pub theme: Theme,

    pub screen: Screen,
    pub should_quit: bool,
    pub status_message: Option<String>,

    pub namespaces: Vec<String>,
    pub namespace_filter: Option<String>, // None = all namespaces

    pub workload_kind: WorkloadKind,
    pub pods: Vec<PodSummary>,
    pub nodes: Vec<NodeSummary>,
    pub workloads_cache: HashMap<&'static str, Vec<WorkloadSummary>>,
    /// Identity of the currently-selected Workloads/Attention row — kept
    /// stable across list refreshes even when the underlying API returns
    /// items in a different order (which it does, sometimes). This is the
    /// source of truth; `workload_list_state`'s index is recomputed from
    /// it every render via `resolve_selection` and exists only because
    /// ratatui's stateful `List` widget needs one.
    pub selected: Option<ItemRef>,
    pub workload_list_state: ListState,

    pub events: Vec<EventSummary>,

    pub logs: LogViewerState,
    pub object_map: ObjectMapState,
    pub compare: CompareState,
    pub palette: PaletteState,

    /// Forwards started this session, most recent first — just for display;
    /// they run until the app exits (see `k8s::portforward`).
    pub port_forwards: Vec<String>,
    pub pending_shell: Option<(String, String, Option<String>)>, // set to request a suspend+exec
    pub debug_prompt: Option<DebugContainerPrompt>,
}

/// State for the "type an image, launch a debug container, attach a shell"
/// flow — `S` on a pod opens this, Enter submits it.
pub struct DebugContainerPrompt {
    pub namespace: String,
    pub pod: String,
    pub image: String,
}

impl App {
    pub fn new(config: Config) -> Self {
        App {
            config,
            contexts: vec![],
            kubeconfig_files: HashMap::new(),
            context_selected: 0,
            client: None,
            current_context_name: None,
            theme: Theme::default(),
            screen: Screen::ContextPicker,
            should_quit: false,
            status_message: None,
            namespaces: vec![],
            namespace_filter: None,
            workload_kind: WorkloadKind::Pods,
            pods: vec![],
            nodes: vec![],
            workloads_cache: HashMap::new(),
            selected: None,
            workload_list_state: ListState::default(),
            events: vec![],
            logs: LogViewerState::default(),
            object_map: ObjectMapState::default(),
            compare: CompareState::default(),
            palette: PaletteState::default(),
            port_forwards: vec![],
            pending_shell: None,
            debug_prompt: None,
        }
    }

    pub fn set_status(&mut self, msg: impl Into<String>) {
        self.status_message = Some(msg.into());
    }

    pub fn next_screen(&mut self) {
        let idx = MAIN_SCREENS
            .iter()
            .position(|s| *s == self.screen)
            .unwrap_or(0);
        self.screen = MAIN_SCREENS[(idx + 1) % MAIN_SCREENS.len()];
    }

    pub fn prev_screen(&mut self) {
        let idx = MAIN_SCREENS
            .iter()
            .position(|s| *s == self.screen)
            .unwrap_or(0);
        self.screen = MAIN_SCREENS[(idx + MAIN_SCREENS.len() - 1) % MAIN_SCREENS.len()];
    }

    pub fn active_workloads(&self) -> Vec<WorkloadSummary> {
        match self.workload_kind {
            WorkloadKind::Pods => self
                .pods
                .iter()
                .map(|p| WorkloadSummary {
                    kind: "Pod",
                    name: p.name.clone(),
                    namespace: p.namespace.clone(),
                    ready: format!("{}/{}", p.ready.0, p.ready.1),
                })
                .collect(),
            WorkloadKind::Nodes => self
                .nodes
                .iter()
                .map(|n| WorkloadSummary {
                    kind: "Node",
                    name: n.name.clone(),
                    namespace: String::new(),
                    ready: if n.ready {
                        "Ready".into()
                    } else {
                        "NotReady".into()
                    },
                })
                .collect(),
            other => self
                .workloads_cache
                .get(other.api_kind())
                .cloned()
                .unwrap_or_default(),
        }
    }

    /// The identity of every row rendered on the Workloads/Attention screen
    /// right now, in display order. `attention_only` selects the Attention
    /// screen's filtered pod list instead of the current workload kind's
    /// full list — the two can have different lengths, so bounding
    /// selection movement or resolving "what's highlighted" against the
    /// wrong one silently targets the wrong item.
    pub fn visible_item_refs(&self, attention_only: bool) -> Vec<ItemRef> {
        if attention_only {
            return crate::k8s::resources::filter_attention(&self.pods)
                .iter()
                .map(|p| ItemRef {
                    kind: "Pod".into(),
                    namespace: p.namespace.clone(),
                    name: p.name.clone(),
                })
                .collect();
        }
        self.active_workloads()
            .iter()
            .map(|w| ItemRef {
                kind: w.kind.to_string(),
                namespace: w.namespace.clone(),
                name: w.name.clone(),
            })
            .collect()
    }

    /// Finds `self.selected` in `refs` and returns its index. If it's not
    /// there — nothing was selected yet, the list reordered and lost it,
    /// or the item itself is gone — falls back to index 0 and adopts *that*
    /// row's identity, so a stale reference never silently survives as the
    /// active selection. Returns `None` only when `refs` is empty.
    pub fn resolve_selection(&mut self, refs: &[ItemRef]) -> Option<usize> {
        if let Some(sel) = &self.selected {
            if let Some(idx) = refs.iter().position(|r| r == sel) {
                return Some(idx);
            }
        }
        if refs.is_empty() {
            self.selected = None;
            None
        } else {
            self.selected = Some(refs[0].clone());
            Some(0)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn screen_cycling_wraps_in_both_directions() {
        let mut app = App::new(Config::default());
        app.screen = Screen::Overview;

        for _ in 0..MAIN_SCREENS.len() {
            app.next_screen();
        }
        assert_eq!(
            app.screen,
            Screen::Overview,
            "a full lap forward returns to the start"
        );

        app.prev_screen();
        assert_eq!(
            app.screen,
            *MAIN_SCREENS.last().unwrap(),
            "stepping back from the start wraps to the end"
        );
    }

    #[test]
    fn workload_kind_cache_key_matches_the_resource_kind_string() {
        // active_workloads() looks entries up by api_kind(); if that ever
        // drifts from what k8s::resources listers actually produce, cached
        // workloads silently stop showing up.
        assert_eq!(WorkloadKind::Deployments.api_kind(), "Deployment");
        assert_eq!(WorkloadKind::CronJobs.api_kind(), "CronJob");
    }

    fn item(name: &str) -> ItemRef {
        ItemRef {
            kind: "Pod".into(),
            namespace: "default".into(),
            name: name.into(),
        }
    }

    #[test]
    fn resolve_selection_follows_an_item_across_a_reorder() {
        // Regression test: a background refresh can come back with the
        // same items in a different order (observed against a real
        // cluster). Selection must follow the *item*, not the slot.
        let mut app = App::new(Config::default());
        let before = [item("a"), item("b"), item("c")];
        assert_eq!(app.resolve_selection(&before), Some(0));

        app.selected = Some(item("b"));
        let after_reorder = [item("c"), item("b"), item("a")];
        assert_eq!(
            app.resolve_selection(&after_reorder),
            Some(1),
            "selection should follow 'b' to its new slot, not stay at index 0"
        );
        assert_eq!(app.selected, Some(item("b")));
    }

    #[test]
    fn resolve_selection_falls_back_to_first_row_when_selection_is_gone() {
        let mut app = App::new(Config::default());
        app.selected = Some(item("deleted"));
        let refs = [item("a"), item("b")];
        assert_eq!(app.resolve_selection(&refs), Some(0));
        assert_eq!(
            app.selected,
            Some(item("a")),
            "must adopt the fallback row's identity, not keep pointing at the gone one"
        );
    }

    #[test]
    fn resolve_selection_on_empty_list_clears_selection() {
        let mut app = App::new(Config::default());
        app.selected = Some(item("a"));
        assert_eq!(app.resolve_selection(&[]), None);
        assert_eq!(app.selected, None);
    }
}
