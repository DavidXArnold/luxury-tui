//! Port forwarding to a pod, the same "right click a pod -> forward a port"
//! convenience Luxury Yacht offers, minus the mouse.
//!
//! Kept intentionally simple: once started, a forward runs for the life of
//! the app (it stops when the pod's connection drops or the app exits).
//! There's no cancellation handle to manage — one less moving part.

use anyhow::{Context as _, Result};
use k8s_openapi::api::core::v1::Pod;
use kube::{Api, Client};
use tokio::net::TcpListener;

#[derive(Debug, Clone)]
pub struct PortForwardInfo {
    pub local_port: u16,
    pub remote_port: u16,
    pub pod: String,
    pub namespace: String,
}

/// Starts forwarding `local_port` (0 = pick any free port) on localhost to
/// `remote_port` on the given pod. Each accepted local connection opens its
/// own stream to the pod, same as `kubectl port-forward`.
pub async fn start(
    client: Client,
    namespace: String,
    pod: String,
    local_port: u16,
    remote_port: u16,
) -> Result<PortForwardInfo> {
    let listener = TcpListener::bind(("127.0.0.1", local_port))
        .await
        .context("binding local port-forward listener")?;
    let bound_port = listener.local_addr()?.port();

    tokio::spawn(accept_loop(
        client,
        namespace.clone(),
        pod.clone(),
        remote_port,
        listener,
    ));

    Ok(PortForwardInfo {
        local_port: bound_port,
        remote_port,
        pod,
        namespace,
    })
}

async fn accept_loop(
    client: Client,
    namespace: String,
    pod: String,
    remote_port: u16,
    listener: TcpListener,
) {
    loop {
        let Ok((tcp_stream, _addr)) = listener.accept().await else {
            return;
        };
        let client = client.clone();
        let namespace = namespace.clone();
        let pod = pod.clone();
        tokio::spawn(async move {
            if let Err(e) = forward_one(client, &namespace, &pod, remote_port, tcp_stream).await {
                tracing::debug!("port-forward connection ended: {e}");
            }
        });
    }
}

async fn forward_one(
    client: Client,
    namespace: &str,
    pod: &str,
    remote_port: u16,
    mut tcp_stream: tokio::net::TcpStream,
) -> Result<()> {
    let api: Api<Pod> = Api::namespaced(client, namespace);
    let mut pf = api.portforward(pod, &[remote_port]).await?;
    let mut upstream = pf
        .take_stream(remote_port)
        .context("kube did not open a stream for the requested port")?;
    tokio::io::copy_bidirectional(&mut tcp_stream, &mut upstream).await?;
    Ok(())
}
