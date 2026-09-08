//! Spawns background async work and reports results back over the event
//! channel, keeping the render loop non-blocking.

use kube::Client;

use crate::event::{AppEvent, EventSender};
use crate::k8s::{self, logs::LogRequest};

pub fn refresh_namespaces(client: Client, tx: EventSender) {
    tokio::spawn(async move {
        let result = k8s::resources::list_namespaces(&client).await;
        let _ = tx.send(AppEvent::NamespacesLoaded(result));
    });
}

pub fn refresh_pods(client: Client, namespace: Option<String>, tx: EventSender) {
    tokio::spawn(async move {
        let result = k8s::resources::list_pods(&client, namespace.as_deref()).await;
        let _ = tx.send(AppEvent::PodsLoaded(result));
    });
}

pub fn refresh_nodes(client: Client, tx: EventSender) {
    tokio::spawn(async move {
        let result = k8s::resources::list_nodes(&client).await;
        let _ = tx.send(AppEvent::NodesLoaded(result));
    });
}

pub fn refresh_events(client: Client, namespace: Option<String>, tx: EventSender) {
    tokio::spawn(async move {
        let result = k8s::resources::list_events(&client, namespace.as_deref()).await;
        let _ = tx.send(AppEvent::EventsLoaded(result));
    });
}

macro_rules! spawn_workload_refresh {
    ($fn_name:ident, $lister:path, $kind:expr) => {
        pub fn $fn_name(client: Client, namespace: Option<String>, tx: EventSender) {
            tokio::spawn(async move {
                let result = $lister(&client, namespace.as_deref()).await;
                let _ = tx.send(AppEvent::WorkloadsLoaded($kind, result));
            });
        }
    };
}

spawn_workload_refresh!(
    refresh_deployments,
    k8s::resources::list_deployments,
    "Deployment"
);
spawn_workload_refresh!(
    refresh_statefulsets,
    k8s::resources::list_statefulsets,
    "StatefulSet"
);
spawn_workload_refresh!(
    refresh_daemonsets,
    k8s::resources::list_daemonsets,
    "DaemonSet"
);
spawn_workload_refresh!(
    refresh_replicasets,
    k8s::resources::list_replicasets,
    "ReplicaSet"
);
spawn_workload_refresh!(refresh_jobs, k8s::resources::list_jobs, "Job");
spawn_workload_refresh!(refresh_cronjobs, k8s::resources::list_cronjobs, "CronJob");
spawn_workload_refresh!(refresh_services, k8s::resources::list_services, "Service");

pub fn start_log_stream(client: Client, req: LogRequest, tx: EventSender) {
    tokio::spawn(async move {
        let mut rx = k8s::logs::stream_logs(client, req);
        while let Some(line) = rx.recv().await {
            if tx.send(AppEvent::LogLine(line)).is_err() {
                return;
            }
        }
        let _ = tx.send(AppEvent::LogStreamEnded);
    });
}

pub fn load_object_map(client: Client, namespace: String, pod: String, tx: EventSender) {
    tokio::spawn(async move {
        let result = k8s::objectmap::map_for_pod(&client, &namespace, &pod).await;
        let _ = tx.send(AppEvent::ObjectMapLoaded(result));
    });
}

pub fn drain_node(client: Client, node_name: String, tx: EventSender) {
    tokio::spawn(async move {
        let result = k8s::nodes::drain(&client, &node_name).await;
        let _ = tx.send(AppEvent::DrainFinished(result));
    });
}

pub fn cordon_node(client: Client, node_name: String, unschedulable: bool, tx: EventSender) {
    tokio::spawn(async move {
        let result = k8s::nodes::cordon(&client, &node_name, unschedulable)
            .await
            .map(|_| {
                format!(
                    "node/{node_name} {}",
                    if unschedulable {
                        "cordoned"
                    } else {
                        "uncordoned"
                    }
                )
            });
        let _ = tx.send(AppEvent::ActionResult(result));
    });
}

pub fn compare_pods(
    client: Client,
    left: (String, String),
    right: (String, String),
    tx: EventSender,
) {
    tokio::spawn(async move {
        let result = k8s::diff::compare_pods(
            &client,
            (left.0.as_str(), left.1.as_str()),
            (right.0.as_str(), right.1.as_str()),
        )
        .await;
        let _ = tx.send(AppEvent::DiffReady(result));
    });
}

/// Forwards a pod's first container port (or 8080 if it has none declared)
/// to a free local port.
pub fn start_port_forward(client: Client, namespace: String, pod: String, tx: EventSender) {
    tokio::spawn(async move {
        let remote_port = k8s::resources::first_container_port(&client, &namespace, &pod)
            .await
            .unwrap_or(8080);
        let result = k8s::portforward::start(client, namespace, pod, 0, remote_port)
            .await
            .map(|info| {
                format!(
                    "127.0.0.1:{} -> {}/{}:{}",
                    info.local_port, info.namespace, info.pod, info.remote_port
                )
            });
        let _ = tx.send(AppEvent::PortForwardStarted(result));
    });
}

pub fn delete_node(client: Client, node_name: String, tx: EventSender) {
    tokio::spawn(async move {
        let result = k8s::nodes::delete_node(&client, &node_name)
            .await
            .map(|_| format!("node/{node_name} deleted"));
        let _ = tx.send(AppEvent::ActionResult(result));
    });
}
