//! "Object Comparison": diff two resources (e.g. the same workload across
//! two namespaces, or two pods suspected of configuration drift) by
//! pretty-printing their JSON and running a text diff over it.

use anyhow::{bail, Result};
use k8s_openapi::api::apps::v1::{DaemonSet, Deployment, ReplicaSet, StatefulSet};
use k8s_openapi::api::batch::v1::{CronJob, Job};
use k8s_openapi::api::core::v1::{Node, Pod, Service};
use kube::{Api, Client};
use serde::de::DeserializeOwned;
use serde::Serialize;
use serde_json::Value;
use similar::{ChangeTag, TextDiff};

#[derive(Debug, Clone)]
pub enum DiffLine {
    Context(String),
    Added(String),
    Removed(String),
}

fn strip_noise(mut value: Value) -> Value {
    if let Some(obj) = value.as_object_mut() {
        obj.remove("managedFields");
        if let Some(meta) = obj.get_mut("metadata").and_then(|m| m.as_object_mut()) {
            meta.remove("managedFields");
            meta.remove("resourceVersion");
            meta.remove("uid");
            meta.remove("generation");
            meta.remove("creationTimestamp");
        }
        obj.remove("status");
    }
    value
}

/// Fetches two resources of the same kind (by namespace/name — namespace is
/// ignored for cluster-scoped kinds like Node) and returns a line-by-line
/// diff of them, with the noisy, always-different metadata stripped out.
///
/// `kind` matches `app::WorkloadKind::api_kind()` — "Pod", "Deployment",
/// "StatefulSet", "DaemonSet", "ReplicaSet", "Job", "CronJob", "Service", or
/// "Node".
pub async fn compare_resources(
    client: &Client,
    kind: &str,
    left: (&str, &str),
    right: (&str, &str),
) -> Result<Vec<DiffLine>> {
    let a = fetch_resource_json(client, kind, left.0, left.1).await?;
    let b = fetch_resource_json(client, kind, right.0, right.1).await?;
    Ok(diff_json(&a, &b))
}

async fn fetch_resource_json(
    client: &Client,
    kind: &str,
    namespace: &str,
    name: &str,
) -> Result<Value> {
    let value = match kind {
        "Pod" => fetch_namespaced::<Pod>(client, namespace, name).await?,
        "Deployment" => fetch_namespaced::<Deployment>(client, namespace, name).await?,
        "StatefulSet" => fetch_namespaced::<StatefulSet>(client, namespace, name).await?,
        "DaemonSet" => fetch_namespaced::<DaemonSet>(client, namespace, name).await?,
        "ReplicaSet" => fetch_namespaced::<ReplicaSet>(client, namespace, name).await?,
        "Job" => fetch_namespaced::<Job>(client, namespace, name).await?,
        "CronJob" => fetch_namespaced::<CronJob>(client, namespace, name).await?,
        "Service" => fetch_namespaced::<Service>(client, namespace, name).await?,
        "Node" => fetch_cluster_scoped::<Node>(client, name).await?,
        other => bail!("comparison isn't supported for kind '{other}'"),
    };
    Ok(strip_noise(value))
}

async fn fetch_namespaced<K>(client: &Client, namespace: &str, name: &str) -> Result<Value>
where
    K: kube::Resource<DynamicType = (), Scope = kube::core::NamespaceResourceScope>
        + Clone
        + DeserializeOwned
        + Serialize
        + std::fmt::Debug,
{
    let api: Api<K> = Api::namespaced(client.clone(), namespace);
    Ok(serde_json::to_value(api.get(name).await?)?)
}

async fn fetch_cluster_scoped<K>(client: &Client, name: &str) -> Result<Value>
where
    K: kube::Resource<DynamicType = ()> + Clone + DeserializeOwned + Serialize + std::fmt::Debug,
{
    let api: Api<K> = Api::all(client.clone());
    Ok(serde_json::to_value(api.get(name).await?)?)
}

pub fn diff_json(a: &Value, b: &Value) -> Vec<DiffLine> {
    let a_text = serde_json::to_string_pretty(a).unwrap_or_default();
    let b_text = serde_json::to_string_pretty(b).unwrap_or_default();
    let diff = TextDiff::from_lines(&a_text, &b_text);
    diff.iter_all_changes()
        .map(|change| {
            let text = change.to_string_lossy().trim_end().to_string();
            match change.tag() {
                ChangeTag::Delete => DiffLine::Removed(text),
                ChangeTag::Insert => DiffLine::Added(text),
                ChangeTag::Equal => DiffLine::Context(text),
            }
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn strip_noise_removes_fields_that_always_differ() {
        let value = json!({
            "metadata": {
                "name": "web",
                "resourceVersion": "12345",
                "uid": "abc-def",
                "generation": 3,
                "creationTimestamp": "2024-01-01T00:00:00Z"
            },
            "status": { "phase": "Running" },
            "managedFields": [{"manager": "kubectl"}]
        });

        let cleaned = strip_noise(value);

        assert_eq!(cleaned["metadata"]["name"], "web");
        assert!(cleaned.get("status").is_none());
        assert!(cleaned.get("managedFields").is_none());
        assert!(cleaned["metadata"].get("resourceVersion").is_none());
        assert!(cleaned["metadata"].get("uid").is_none());
    }

    #[test]
    fn diff_json_reports_added_and_removed_lines() {
        let a = json!({ "spec": { "replicas": 1 } });
        let b = json!({ "spec": { "replicas": 2 } });

        let lines = diff_json(&a, &b);

        assert!(lines
            .iter()
            .any(|l| matches!(l, DiffLine::Removed(s) if s.contains('1'))));
        assert!(lines
            .iter()
            .any(|l| matches!(l, DiffLine::Added(s) if s.contains('2'))));
    }

    #[test]
    fn diff_json_of_identical_values_has_no_changes() {
        let a = json!({ "spec": { "replicas": 3 } });
        let lines = diff_json(&a, &a.clone());
        assert!(lines.iter().all(|l| matches!(l, DiffLine::Context(_))));
    }
}
