//! Typed listers for the resource kinds Luxury TUI understands, plus the
//! lightweight summary structs the UI renders from.

use anyhow::Result;
use k8s_openapi::api::apps::v1::{DaemonSet, Deployment, ReplicaSet, StatefulSet};
use k8s_openapi::api::batch::v1::{CronJob, Job};
use k8s_openapi::api::core::v1::{Event, Namespace, Node, Pod, Service};
use kube::api::ListParams;
use kube::{Api, Client, ResourceExt};

#[derive(Debug, Clone)]
pub struct PodSummary {
    pub name: String,
    pub namespace: String,
    pub phase: String,
    pub ready: (u32, u32),
    pub restarts: i32,
    pub node: Option<String>,
    pub owner_kind: Option<String>,
    pub owner_name: Option<String>,
    pub age_seconds: i64,
}

#[derive(Debug, Clone)]
pub struct NodeSummary {
    pub name: String,
    pub ready: bool,
    pub schedulable: bool,
    pub roles: String,
    pub version: String,
}

#[derive(Debug, Clone)]
pub struct WorkloadSummary {
    pub kind: &'static str,
    pub name: String,
    pub namespace: String,
    pub ready: String,
}

#[derive(Debug, Clone)]
pub struct EventSummary {
    pub namespace: String,
    pub kind: String,
    pub name: String,
    pub reason: String,
    pub message: String,
    pub type_: String,
    pub count: i32,
}

pub async fn list_namespaces(client: &Client) -> Result<Vec<String>> {
    let api: Api<Namespace> = Api::all(client.clone());
    let list = api.list(&ListParams::default()).await?;
    Ok(list.items.into_iter().map(|ns| ns.name_any()).collect())
}

pub async fn list_nodes(client: &Client) -> Result<Vec<NodeSummary>> {
    let api: Api<Node> = Api::all(client.clone());
    let list = api.list(&ListParams::default()).await?;
    Ok(list
        .items
        .into_iter()
        .map(|n| {
            let ready = n
                .status
                .as_ref()
                .and_then(|s| s.conditions.as_ref())
                .map(|conds| {
                    conds
                        .iter()
                        .any(|c| c.type_ == "Ready" && c.status == "True")
                })
                .unwrap_or(false);
            let schedulable = !n
                .spec
                .as_ref()
                .and_then(|s| s.unschedulable)
                .unwrap_or(false);
            let roles = n
                .labels()
                .keys()
                .filter_map(|k| k.strip_prefix("node-role.kubernetes.io/"))
                .collect::<Vec<_>>()
                .join(",");
            let roles = if roles.is_empty() {
                "<none>".to_string()
            } else {
                roles
            };
            let version = n
                .status
                .as_ref()
                .and_then(|s| s.node_info.as_ref())
                .map(|i| i.kubelet_version.clone())
                .unwrap_or_default();
            NodeSummary {
                name: n.name_any(),
                ready,
                schedulable,
                roles,
                version,
            }
        })
        .collect())
}

fn pod_age_seconds(pod: &Pod) -> i64 {
    pod.metadata
        .creation_timestamp
        .as_ref()
        .map(|t| (chrono::Utc::now() - t.0).num_seconds())
        .unwrap_or(0)
}

fn pod_owner(pod: &Pod) -> (Option<String>, Option<String>) {
    pod.owner_references()
        .first()
        .map(|o| (Some(o.kind.clone()), Some(o.name.clone())))
        .unwrap_or((None, None))
}

pub async fn list_pods(client: &Client, namespace: Option<&str>) -> Result<Vec<PodSummary>> {
    let api: Api<Pod> = match namespace {
        Some(ns) => Api::namespaced(client.clone(), ns),
        None => Api::all(client.clone()),
    };
    let list = api.list(&ListParams::default()).await?;
    Ok(list
        .items
        .iter()
        .map(|pod| {
            let phase = pod
                .status
                .as_ref()
                .and_then(|s| s.phase.clone())
                .unwrap_or_else(|| "Unknown".to_string());
            let statuses = pod
                .status
                .as_ref()
                .and_then(|s| s.container_statuses.clone())
                .unwrap_or_default();
            let ready_count = statuses.iter().filter(|c| c.ready).count() as u32;
            let total = statuses.len() as u32;
            let restarts = statuses.iter().map(|c| c.restart_count).sum();
            let (owner_kind, owner_name) = pod_owner(pod);
            PodSummary {
                name: pod.name_any(),
                namespace: pod.namespace().unwrap_or_default(),
                phase,
                ready: (ready_count, total),
                restarts,
                node: pod.spec.as_ref().and_then(|s| s.node_name.clone()),
                owner_kind,
                owner_name,
                age_seconds: pod_age_seconds(pod),
            }
        })
        .collect())
}

fn needs_attention(p: &PodSummary) -> bool {
    !matches!(p.phase.as_str(), "Running" | "Succeeded") || p.restarts >= 5 || p.ready.0 < p.ready.1
}

/// Pods that need attention: not Running/Succeeded, or restarting a lot.
pub fn filter_attention(pods: &[PodSummary]) -> Vec<PodSummary> {
    pods.iter()
        .filter(|p| needs_attention(p))
        .cloned()
        .collect()
}

/// Same filter as `filter_attention`, without cloning every match — for
/// when only the count is needed (the Attention screen's side panel shows
/// this next to the full filtered+cloned list `filter_attention` builds
/// for the main pane; at thousands of pods, doing the full clone twice
/// just to also read `.len()` is pointless work).
pub fn attention_count(pods: &[PodSummary]) -> usize {
    pods.iter().filter(|p| needs_attention(p)).count()
}

macro_rules! workload_lister {
    ($fn_name:ident, $ty:ty, $kind:expr, $ready:expr) => {
        pub async fn $fn_name(
            client: &Client,
            namespace: Option<&str>,
        ) -> Result<Vec<WorkloadSummary>> {
            let api: Api<$ty> = match namespace {
                Some(ns) => Api::namespaced(client.clone(), ns),
                None => Api::all(client.clone()),
            };
            let list = api.list(&ListParams::default()).await?;
            Ok(list
                .items
                .into_iter()
                .map(|item| {
                    let namespace = item.namespace().unwrap_or_default();
                    let name = item.name_any();
                    let ready = $ready(&item);
                    WorkloadSummary {
                        kind: $kind,
                        name,
                        namespace,
                        ready,
                    }
                })
                .collect())
        }
    };
}

workload_lister!(
    list_deployments,
    Deployment,
    "Deployment",
    |d: &Deployment| {
        let status = d.status.as_ref();
        format!(
            "{}/{}",
            status.and_then(|s| s.ready_replicas).unwrap_or(0),
            status.and_then(|s| s.replicas).unwrap_or(0)
        )
    }
);

workload_lister!(
    list_statefulsets,
    StatefulSet,
    "StatefulSet",
    |s: &StatefulSet| {
        let status = s.status.as_ref();
        format!(
            "{}/{}",
            status.map(|s| s.ready_replicas.unwrap_or(0)).unwrap_or(0),
            status.map(|s| s.replicas).unwrap_or(0)
        )
    }
);

workload_lister!(list_daemonsets, DaemonSet, "DaemonSet", |d: &DaemonSet| {
    let status = d.status.as_ref();
    format!(
        "{}/{}",
        status.map(|s| s.number_ready).unwrap_or(0),
        status.map(|s| s.desired_number_scheduled).unwrap_or(0)
    )
});

workload_lister!(
    list_replicasets,
    ReplicaSet,
    "ReplicaSet",
    |r: &ReplicaSet| {
        let status = r.status.as_ref();
        format!(
            "{}/{}",
            status.and_then(|s| s.ready_replicas).unwrap_or(0),
            status.map(|s| s.replicas).unwrap_or(0)
        )
    }
);

workload_lister!(list_jobs, Job, "Job", |j: &Job| {
    let status = j.status.as_ref();
    format!(
        "{}/{}",
        status.and_then(|s| s.succeeded).unwrap_or(0),
        j.spec.as_ref().and_then(|s| s.completions).unwrap_or(1)
    )
});

workload_lister!(list_cronjobs, CronJob, "CronJob", |c: &CronJob| {
    let suspended = c.spec.as_ref().and_then(|s| s.suspend).unwrap_or(false);
    if suspended {
        "suspended".to_string()
    } else {
        "scheduled".to_string()
    }
});

workload_lister!(list_services, Service, "Service", |s: &Service| {
    s.spec
        .as_ref()
        .and_then(|s| s.type_.clone())
        .unwrap_or_else(|| "ClusterIP".to_string())
});

/// Fetches Warning-type events (the only kind anything here ever displays
/// — see `warning_events`) via a server-side field selector rather than
/// pulling every event and filtering client-side. In a busy cluster,
/// Normal events (pod scheduled, image pulled, container started, ...)
/// vastly outnumber Warnings; there's no reason to ship all of them over
/// the wire just to throw most away on arrival.
pub async fn list_events(client: &Client, namespace: Option<&str>) -> Result<Vec<EventSummary>> {
    let params = ListParams::default().fields("type=Warning");
    let api: Api<Event> = match namespace {
        Some(ns) => Api::namespaced(client.clone(), ns),
        None => Api::all(client.clone()),
    };
    let mut list = api.list(&params).await?;
    list.items
        .sort_by_key(|e| std::cmp::Reverse(e.count.unwrap_or(0)));
    Ok(list
        .items
        .into_iter()
        .map(|e| EventSummary {
            namespace: e.namespace().unwrap_or_default(),
            kind: e.involved_object.kind.clone().unwrap_or_default(),
            name: e.involved_object.name.clone().unwrap_or_default(),
            reason: e.reason.clone().unwrap_or_default(),
            message: e.message.clone().unwrap_or_default(),
            type_: e.type_.clone().unwrap_or_default(),
            count: e.count.unwrap_or(0),
        })
        .collect())
}

pub fn warning_events(events: &[EventSummary]) -> Vec<EventSummary> {
    events
        .iter()
        .filter(|e| e.type_ == "Warning")
        .cloned()
        .collect()
}

/// The first declared container port on a pod, if any — used as a sane
/// default target for "forward this pod" without asking the user first.
pub async fn first_container_port(client: &Client, namespace: &str, pod: &str) -> Option<u16> {
    let api: Api<Pod> = Api::namespaced(client.clone(), namespace);
    let pod = api.get(pod).await.ok()?;
    pod.spec?
        .containers
        .into_iter()
        .find_map(|c| c.ports?.first().map(|p| p.container_port as u16))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn healthy_pod(name: &str) -> PodSummary {
        PodSummary {
            name: name.into(),
            namespace: "default".into(),
            phase: "Running".into(),
            ready: (1, 1),
            restarts: 0,
            node: None,
            owner_kind: None,
            owner_name: None,
            age_seconds: 60,
        }
    }

    #[test]
    fn healthy_pods_do_not_need_attention() {
        let pods = vec![healthy_pod("web-1")];
        assert!(filter_attention(&pods).is_empty());
    }

    #[test]
    fn pending_pods_need_attention() {
        let mut pod = healthy_pod("web-1");
        pod.phase = "Pending".into();
        assert_eq!(filter_attention(&[pod]).len(), 1);
    }

    #[test]
    fn heavily_restarting_pods_need_attention() {
        let mut pod = healthy_pod("web-1");
        pod.restarts = 12;
        assert_eq!(filter_attention(&[pod]).len(), 1);
    }

    #[test]
    fn not_fully_ready_pods_need_attention() {
        let mut pod = healthy_pod("web-1");
        pod.ready = (0, 1);
        assert_eq!(filter_attention(&[pod]).len(), 1);
    }

    #[test]
    fn attention_count_always_matches_filter_attention_length() {
        // attention_count exists purely so the Attention screen's side
        // panel doesn't have to clone the whole matching set just to read
        // its length — they must never disagree.
        let mut pending = healthy_pod("pending-1");
        pending.phase = "Pending".into();
        let mut flapping = healthy_pod("flapping-1");
        flapping.restarts = 9;
        let pods = vec![healthy_pod("ok-1"), pending, flapping, healthy_pod("ok-2")];

        assert_eq!(attention_count(&pods), filter_attention(&pods).len());
        assert_eq!(attention_count(&pods), 2);
    }
}
