//! Integration tests against a real Kubernetes API server.
//!
//! In CI (see `.github/workflows/ci.yml`, job `integration`) these run
//! against a `kind` cluster with `kwok`'s controller installed alongside
//! the real one, so we get real pod scheduling *and* cheap fake nodes in
//! the same cluster.
//!
//! Skipped by default (including in the plain `cargo test` job) unless
//! `LUXURY_TUI_INTEGRATION=1` is set, since they need a live cluster and a
//! `KUBECONFIG` pointed at it. To run locally against your own `kind`
//! cluster with kwok installed:
//!
//!     LUXURY_TUI_INTEGRATION=1 cargo test --test cluster_integration
//!
//! Resources these tests create are left behind rather than cleaned up —
//! the CI kind cluster is thrown away at the end of the job either way, so
//! there's nothing to gain from the extra bookkeeping.

use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use k8s_openapi::api::apps::v1::Deployment;
use k8s_openapi::api::core::v1::{Namespace, Node, Pod};
use kube::api::PostParams;
use kube::{Api, Client};
use serde_json::json;

use luxury_tui::k8s::{nodes, objectmap, resources};

fn integration_enabled() -> bool {
    std::env::var("LUXURY_TUI_INTEGRATION").as_deref() == Ok("1")
}

/// A short, good-enough-to-avoid-collisions suffix so parallel test runs
/// don't step on each other's namespaces/nodes.
fn unique(prefix: &str) -> String {
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    format!("{prefix}-{nanos}")
}

async fn client() -> Client {
    Client::try_default()
        .await
        .expect("connect to the test cluster — is KUBECONFIG set?")
}

async fn create_namespace(client: &Client, name: &str) {
    let api: Api<Namespace> = Api::all(client.clone());
    let ns = serde_json::from_value(json!({
        "apiVersion": "v1",
        "kind": "Namespace",
        "metadata": { "name": name }
    }))
    .unwrap();
    api.create(&PostParams::default(), &ns)
        .await
        .expect("create test namespace");
}

#[tokio::test]
async fn namespaces_include_the_builtin_ones() {
    if !integration_enabled() {
        return;
    }
    let client = client().await;
    let namespaces = resources::list_namespaces(&client)
        .await
        .expect("list namespaces");
    assert!(namespaces.iter().any(|n| n == "default"));
    assert!(namespaces.iter().any(|n| n == "kube-system"));
}

#[tokio::test]
async fn the_kind_control_plane_node_is_visible_and_ready() {
    if !integration_enabled() {
        return;
    }
    let client = client().await;
    let all_nodes = resources::list_nodes(&client).await.expect("list nodes");
    assert!(!all_nodes.is_empty());
    assert!(
        all_nodes.iter().any(|n| n.ready),
        "expected at least one Ready node"
    );
}

#[tokio::test]
async fn a_real_pod_leaves_attention_once_it_is_running() {
    if !integration_enabled() {
        return;
    }
    let client = client().await;
    let ns = unique("luxury-tui-it");
    create_namespace(&client, &ns).await;

    let pods_api: Api<Pod> = Api::namespaced(client.clone(), &ns);
    let pod = serde_json::from_value(json!({
        "apiVersion": "v1",
        "kind": "Pod",
        "metadata": { "name": "probe" },
        "spec": {
            "containers": [{ "name": "probe", "image": "busybox:1.36", "command": ["sleep", "3600"] }],
            "restartPolicy": "Never"
        }
    }))
    .unwrap();
    pods_api
        .create(&PostParams::default(), &pod)
        .await
        .expect("create probe pod");

    let deadline = Instant::now() + Duration::from_secs(90);
    loop {
        let pods = resources::list_pods(&client, Some(&ns))
            .await
            .expect("list pods");
        let running_and_healthy = pods.iter().any(|p| p.phase == "Running")
            && resources::filter_attention(&pods).is_empty();
        if running_and_healthy {
            break;
        }
        assert!(
            Instant::now() < deadline,
            "pod never became healthy in time"
        );
        tokio::time::sleep(Duration::from_secs(2)).await;
    }
}

#[tokio::test]
async fn kwok_fake_node_can_be_cordoned_and_uncordoned() {
    if !integration_enabled() {
        return;
    }
    let client = client().await;
    let node_name = unique("kwok-node");

    // kwok's controller watches for the `kwok.x-k8s.io/node` annotation and
    // fakes a kubelet for any Node carrying it, so this node goes Ready
    // without any real hardware behind it. The taint keeps real workloads
    // from ever landing on it — we're only testing node maintenance here.
    let nodes_api: Api<Node> = Api::all(client.clone());
    let node = serde_json::from_value(json!({
        "apiVersion": "v1",
        "kind": "Node",
        "metadata": {
            "name": node_name,
            "annotations": { "kwok.x-k8s.io/node": "fake" },
            "labels": { "type": "kwok", "kubernetes.io/hostname": node_name }
        },
        "spec": {
            "taints": [{ "key": "kwok.x-k8s.io/node", "value": "fake", "effect": "NoSchedule" }]
        },
        "status": {
            "conditions": [{ "type": "Ready", "status": "True", "reason": "KubeletReady", "message": "kwok" }],
            "allocatable": { "cpu": "4", "memory": "8Gi", "pods": "110" },
            "capacity": { "cpu": "4", "memory": "8Gi", "pods": "110" },
            "nodeInfo": { "kubeletVersion": "kwok" }
        }
    }))
    .unwrap();
    nodes_api
        .create(&PostParams::default(), &node)
        .await
        .expect("create kwok node");

    nodes::cordon(&client, &node_name, true)
        .await
        .expect("cordon");
    let after_cordon = resources::list_nodes(&client).await.expect("list nodes");
    let found = after_cordon
        .iter()
        .find(|n| n.name == node_name)
        .expect("kwok node listed");
    assert!(
        !found.schedulable,
        "node should be unschedulable after cordon"
    );

    nodes::cordon(&client, &node_name, false)
        .await
        .expect("uncordon");
    let after_uncordon = resources::list_nodes(&client).await.expect("list nodes");
    let found = after_uncordon
        .iter()
        .find(|n| n.name == node_name)
        .expect("kwok node listed");
    assert!(
        found.schedulable,
        "node should be schedulable again after uncordon"
    );
}

#[tokio::test]
async fn object_map_walks_deployment_to_pod() {
    if !integration_enabled() {
        return;
    }
    let client = client().await;
    let ns = unique("luxury-tui-it");
    create_namespace(&client, &ns).await;

    let deployments_api: Api<Deployment> = Api::namespaced(client.clone(), &ns);
    let deployment = serde_json::from_value(json!({
        "apiVersion": "apps/v1",
        "kind": "Deployment",
        "metadata": { "name": "web" },
        "spec": {
            "replicas": 1,
            "selector": { "matchLabels": { "app": "web" } },
            "template": {
                "metadata": { "labels": { "app": "web" } },
                "spec": {
                    "containers": [{ "name": "web", "image": "busybox:1.36", "command": ["sleep", "3600"] }]
                }
            }
        }
    }))
    .unwrap();
    deployments_api
        .create(&PostParams::default(), &deployment)
        .await
        .expect("create deployment");

    let pods_api: Api<Pod> = Api::namespaced(client.clone(), &ns);
    let deadline = Instant::now() + Duration::from_secs(60);
    let pod_name = loop {
        let pods = pods_api.list(&Default::default()).await.expect("list pods");
        if let Some(p) = pods.items.first() {
            if let Some(name) = &p.metadata.name {
                break name.clone();
            }
        }
        assert!(Instant::now() < deadline, "deployment's pod never appeared");
        tokio::time::sleep(Duration::from_secs(2)).await;
    };

    let map = objectmap::map_for_pod(&client, &ns, &pod_name)
        .await
        .expect("build object map");
    let rendered = objectmap::render_ascii(&map);
    assert!(
        rendered.contains("Deployment web"),
        "expected the Deployment in the map:\n{rendered}"
    );
    assert!(
        rendered.contains("ReplicaSet"),
        "expected a ReplicaSet in the map:\n{rendered}"
    );
    assert!(
        rendered.contains(&pod_name),
        "expected the Pod in the map:\n{rendered}"
    );
}
