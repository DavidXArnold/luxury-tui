//! "Object Maps": walk owner references to build a parent/child tree for a
//! resource, e.g. Deployment -> ReplicaSet -> Pod -> (Service selecting it).

use anyhow::Result;
use k8s_openapi::api::apps::v1::{DaemonSet, Deployment, ReplicaSet, StatefulSet};
use k8s_openapi::api::batch::v1::{CronJob, Job};
use k8s_openapi::api::core::v1::{Pod, Service};
use kube::api::ListParams;
use kube::{Api, Client, ResourceExt};

#[derive(Debug, Clone)]
pub struct MapNode {
    pub kind: String,
    pub name: String,
    // Kept for a future cross-namespace map view; today's render always
    // shows a single namespace's tree so this isn't printed yet.
    #[allow(dead_code)]
    pub namespace: String,
    pub children: Vec<MapNode>,
}

/// Builds a relationship tree rooted at the given Pod: its owning
/// ReplicaSet/Deployment/StatefulSet/DaemonSet/Job/CronJob chain above, and
/// any Services that select it, alongside it.
pub async fn map_for_pod(client: &Client, namespace: &str, pod_name: &str) -> Result<MapNode> {
    let pods_api: Api<Pod> = Api::namespaced(client.clone(), namespace);
    let pod = pods_api.get(pod_name).await?;

    let mut root = MapNode {
        kind: "Pod".into(),
        name: pod_name.to_string(),
        namespace: namespace.to_string(),
        children: vec![],
    };

    // Services selecting this pod (matched by label selector, best-effort).
    let services_api: Api<Service> = Api::namespaced(client.clone(), namespace);
    let services = services_api
        .list(&ListParams::default())
        .await
        .map(|l| l.items)
        .unwrap_or_default();
    let pod_labels = pod.labels();
    for svc in services {
        if let Some(spec) = &svc.spec {
            if let Some(selector) = &spec.selector {
                if !selector.is_empty()
                    && selector.iter().all(|(k, v)| pod_labels.get(k) == Some(v))
                {
                    root.children.push(MapNode {
                        kind: "Service".into(),
                        name: svc.name_any(),
                        namespace: namespace.to_string(),
                        children: vec![],
                    });
                }
            }
        }
    }

    // Owner chain upward, wrapped as a "parent" pseudo-tree by nesting the
    // pod under its owner, and that owner under *its* owner, etc.
    let mut current_kind = pod.owner_references().first().map(|o| o.kind.clone());
    let mut current_name = pod.owner_references().first().map(|o| o.name.clone());
    let mut chain: Vec<(String, String)> = vec![];
    let mut depth = 0;
    while let (Some(kind), Some(name)) = (current_kind.clone(), current_name.clone()) {
        depth += 1;
        if depth > 6 {
            break; // guard against surprises
        }
        chain.push((kind.clone(), name.clone()));
        let next = owner_of(client, namespace, &kind, &name)
            .await
            .ok()
            .flatten();
        match next {
            Some((k, n)) => {
                current_kind = Some(k);
                current_name = Some(n);
            }
            None => break,
        }
    }

    // Fold the chain (furthest ancestor first) into nested parents of root.
    let mut node = root;
    for (kind, name) in chain {
        let parent = MapNode {
            kind,
            name,
            namespace: namespace.to_string(),
            children: vec![node],
        };
        node = parent;
    }

    Ok(node)
}

async fn owner_of(
    client: &Client,
    namespace: &str,
    kind: &str,
    name: &str,
) -> Result<Option<(String, String)>> {
    let owner_refs = match kind {
        "ReplicaSet" => {
            let api: Api<ReplicaSet> = Api::namespaced(client.clone(), namespace);
            api.get(name)
                .await
                .ok()
                .map(|r| r.owner_references().to_vec())
        }
        "Deployment" => {
            let api: Api<Deployment> = Api::namespaced(client.clone(), namespace);
            api.get(name)
                .await
                .ok()
                .map(|r| r.owner_references().to_vec())
        }
        "StatefulSet" => {
            let api: Api<StatefulSet> = Api::namespaced(client.clone(), namespace);
            api.get(name)
                .await
                .ok()
                .map(|r| r.owner_references().to_vec())
        }
        "DaemonSet" => {
            let api: Api<DaemonSet> = Api::namespaced(client.clone(), namespace);
            api.get(name)
                .await
                .ok()
                .map(|r| r.owner_references().to_vec())
        }
        "Job" => {
            let api: Api<Job> = Api::namespaced(client.clone(), namespace);
            api.get(name)
                .await
                .ok()
                .map(|r| r.owner_references().to_vec())
        }
        "CronJob" => {
            let api: Api<CronJob> = Api::namespaced(client.clone(), namespace);
            api.get(name)
                .await
                .ok()
                .map(|r| r.owner_references().to_vec())
        }
        _ => None,
    };
    Ok(owner_refs
        .and_then(|refs| refs.into_iter().next())
        .map(|o| (o.kind, o.name)))
}

pub fn render_ascii(node: &MapNode) -> String {
    let mut out = String::new();
    render_ascii_inner(node, "", true, &mut out);
    out
}

fn render_ascii_inner(node: &MapNode, prefix: &str, is_last: bool, out: &mut String) {
    let branch = if prefix.is_empty() {
        ""
    } else if is_last {
        "\u{2514}\u{2500} "
    } else {
        "\u{251c}\u{2500} "
    };
    out.push_str(prefix);
    out.push_str(branch);
    out.push_str(&format!("{} {}\n", node.kind, node.name));

    let child_prefix = if prefix.is_empty() {
        String::new()
    } else if is_last {
        format!("{prefix}   ")
    } else {
        format!("{prefix}\u{2502}  ")
    };
    let child_prefix = if prefix.is_empty() {
        "   ".to_string()
    } else {
        child_prefix
    };

    for (i, child) in node.children.iter().enumerate() {
        let last = i == node.children.len() - 1;
        render_ascii_inner(child, &child_prefix, last, out);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn leaf(kind: &str, name: &str) -> MapNode {
        MapNode {
            kind: kind.into(),
            name: name.into(),
            namespace: "default".into(),
            children: vec![],
        }
    }

    #[test]
    fn renders_a_flat_chain() {
        let tree = MapNode {
            kind: "Deployment".into(),
            name: "web".into(),
            namespace: "default".into(),
            children: vec![leaf("Pod", "web-abc123")],
        };
        let rendered = render_ascii(&tree);
        assert!(rendered.contains("Deployment web"));
        assert!(rendered.contains("Pod web-abc123"));
        // the child line should be indented under the parent
        let child_line = rendered.lines().find(|l| l.contains("web-abc123")).unwrap();
        assert!(child_line.starts_with("   "));
    }

    #[test]
    fn renders_multiple_children_with_correct_branch_glyphs() {
        let tree = MapNode {
            kind: "Pod".into(),
            name: "web-abc123".into(),
            namespace: "default".into(),
            children: vec![
                leaf("Service", "web-svc"),
                leaf("Service", "web-svc-internal"),
            ],
        };
        let rendered = render_ascii(&tree);
        assert!(rendered.contains('\u{251c}')); // non-last branch
        assert!(rendered.contains('\u{2514}')); // last branch
    }
}
