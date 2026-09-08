use std::collections::HashMap;

use ratatui::widgets::ListState;

use crate::config::Config;
use crate::k8s::context::KubeContext;
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

#[derive(Default)]
pub struct LogViewerState {
    pub lines: Vec<String>,
    pub search: String,
    pub follow: bool,
    pub target: Option<(String, String, Option<String>)>, // namespace, pod, container
    pub scroll: usize,
}

#[derive(Default)]
pub struct CompareState {
    pub left: Option<(String, String)>, // namespace, name
    pub right: Option<(String, String)>,
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
}

impl App {
    pub fn new(config: Config) -> Self {
        App {
            config,
            contexts: vec![],
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
            workload_list_state: ListState::default(),
            events: vec![],
            logs: LogViewerState::default(),
            object_map: ObjectMapState::default(),
            compare: CompareState::default(),
            palette: PaletteState::default(),
            port_forwards: vec![],
            pending_shell: None,
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
}
