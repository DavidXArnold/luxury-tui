//! "Object Comparison": diff two resources (e.g. the same workload across
//! two namespaces, or two pods suspected of configuration drift) by
//! pretty-printing their JSON and running a text diff over it.

use anyhow::Result;
use k8s_openapi::api::core::v1::Pod;
use kube::{Api, Client};
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

/// Fetches two pods (by namespace/name) and returns a line-by-line diff of
/// their specs, with the noisy, always-different metadata stripped out.
pub async fn compare_pods(
    client: &Client,
    left: (&str, &str),
    right: (&str, &str),
) -> Result<Vec<DiffLine>> {
    let a = fetch_pod_json(client, left.0, left.1).await?;
    let b = fetch_pod_json(client, right.0, right.1).await?;
    Ok(diff_json(&a, &b))
}

async fn fetch_pod_json(client: &Client, namespace: &str, name: &str) -> Result<Value> {
    let api: Api<Pod> = Api::namespaced(client.clone(), namespace);
    let pod = api.get(name).await?;
    Ok(strip_noise(serde_json::to_value(pod)?))
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
