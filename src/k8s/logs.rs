//! Streaming pod logs (the "Advanced Log Viewer" feature) — follow, tail,
//! timestamps, delivered line-by-line over a channel the UI polls.

use anyhow::{Context as _, Result};
use futures::{AsyncBufReadExt, StreamExt};
use kube::api::LogParams;
use kube::{Api, Client};
use tokio::sync::mpsc;

#[derive(Debug, Clone)]
pub struct LogRequest {
    pub namespace: String,
    pub pod: String,
    pub container: Option<String>,
    pub follow: bool,
    pub tail_lines: Option<i64>,
    pub timestamps: bool,
    pub previous: bool,
}

/// Spawns a background task streaming logs; returns a receiver of lines.
/// The task exits (and drops the sender) when the stream ends or errors,
/// which the UI can detect via `recv()` returning `None`.
pub fn stream_logs(client: Client, req: LogRequest) -> mpsc::UnboundedReceiver<Result<String>> {
    let (tx, rx) = mpsc::unbounded_channel();
    tokio::spawn(async move {
        let api: Api<k8s_openapi::api::core::v1::Pod> = Api::namespaced(client, &req.namespace);
        let params = LogParams {
            container: req.container.clone(),
            follow: req.follow,
            tail_lines: req.tail_lines,
            timestamps: req.timestamps,
            previous: req.previous,
            ..LogParams::default()
        };
        let stream = match api.log_stream(&req.pod, &params).await {
            Ok(s) => s,
            Err(e) => {
                let _ = tx.send(Err(anyhow::anyhow!(e).context("opening log stream")));
                return;
            }
        };
        let mut lines = stream.lines();
        loop {
            match lines.next().await {
                Some(Ok(line)) => {
                    if tx.send(Ok(line)).is_err() {
                        break; // receiver dropped, UI moved on
                    }
                }
                Some(Err(e)) => {
                    let _ = tx.send(Err(anyhow::Error::from(e).context("reading log stream")));
                    break;
                }
                None => break,
            }
        }
    });
    rx
}

/// Not yet bound to a keybinding — multi-container pods currently just
/// stream the first container; a container picker is next.
#[allow(dead_code)]
pub async fn container_names(client: &Client, namespace: &str, pod: &str) -> Result<Vec<String>> {
    let api: Api<k8s_openapi::api::core::v1::Pod> = Api::namespaced(client.clone(), namespace);
    let pod = api
        .get(pod)
        .await
        .context("fetching pod for container list")?;
    let mut names: Vec<String> = pod
        .spec
        .as_ref()
        .map(|s| s.containers.iter().map(|c| c.name.clone()).collect())
        .unwrap_or_default();
    if let Some(spec) = pod.spec.as_ref() {
        if let Some(init) = &spec.init_containers {
            names.extend(init.iter().map(|c| c.name.clone()));
        }
    }
    Ok(names)
}
