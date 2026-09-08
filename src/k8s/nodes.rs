//! Node maintenance: cordon, drain, delete — same trio Luxury Yacht exposes
//! from a right-click on a node.

use anyhow::{Context as _, Result};
use k8s_openapi::api::core::v1::{Node, Pod};
use kube::api::{Patch, PatchParams};
use kube::{Api, Client, ResourceExt};
use serde_json::json;

pub async fn cordon(client: &Client, node_name: &str, unschedulable: bool) -> Result<()> {
    let api: Api<Node> = Api::all(client.clone());
    let patch = json!({ "spec": { "unschedulable": unschedulable } });
    api.patch(node_name, &PatchParams::default(), &Patch::Merge(&patch))
        .await
        .context("patching node schedulability")?;
    Ok(())
}

pub async fn delete_node(client: &Client, node_name: &str) -> Result<()> {
    let api: Api<Node> = Api::all(client.clone());
    api.delete(node_name, &Default::default())
        .await
        .context("deleting node")?;
    Ok(())
}

#[derive(Debug, Clone)]
pub struct DrainOutcome {
    pub evicted: Vec<String>,
    pub skipped_daemonset: Vec<String>,
    pub failed: Vec<(String, String)>,
}

/// Cordon the node, then evict every non-DaemonSet-managed pod scheduled on it.
pub async fn drain(client: &Client, node_name: &str) -> Result<DrainOutcome> {
    cordon(client, node_name, true).await?;

    let pods_api: Api<Pod> = Api::all(client.clone());
    let all_pods = pods_api
        .list(&Default::default())
        .await
        .context("listing pods for drain")?;

    let mut outcome = DrainOutcome {
        evicted: vec![],
        skipped_daemonset: vec![],
        failed: vec![],
    };

    for pod in all_pods.items {
        let scheduled_here =
            pod.spec.as_ref().and_then(|s| s.node_name.as_deref()) == Some(node_name);
        if !scheduled_here {
            continue;
        }
        let name = pod.name_any();
        let namespace = pod.namespace().unwrap_or_default();
        let is_daemonset = pod.owner_references().iter().any(|o| o.kind == "DaemonSet");
        if is_daemonset {
            outcome.skipped_daemonset.push(name);
            continue;
        }
        let ns_api: Api<Pod> = Api::namespaced(client.clone(), &namespace);
        match ns_api.evict(&name, &Default::default()).await {
            Ok(_) => outcome.evicted.push(name),
            Err(e) => outcome.failed.push((name, e.to_string())),
        }
    }

    Ok(outcome)
}
