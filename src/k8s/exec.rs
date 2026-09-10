//! Shell access into a running container, including spinning up an
//! ephemeral debug container first (Luxury Yacht's "debug container
//! support").

use anyhow::{Context as _, Result};
use k8s_openapi::api::core::v1::{EphemeralContainer, Pod};
use kube::api::{AttachParams, Patch, PatchParams};
use kube::{Api, Client};
use tokio::io::{AsyncReadExt, AsyncWriteExt};

/// Attaches an interactive shell to a container and pipes it to the process's
/// real stdin/stdout until the remote shell exits. Call this only while the
/// TUI's alternate screen / raw-mode wrapper has been temporarily suspended.
pub async fn run_interactive_shell(
    client: &Client,
    namespace: &str,
    pod: &str,
    container: Option<&str>,
    shell: &str,
) -> Result<()> {
    let api: Api<Pod> = Api::namespaced(client.clone(), namespace);
    let mut params = AttachParams::interactive_tty();
    if let Some(c) = container {
        params = params.container(c);
    }
    let mut attached = api
        .exec(pod, vec![shell], &params)
        .await
        .context("starting exec session")?;

    let mut remote_stdout = attached.stdout().context("no stdout on exec session")?;
    let mut remote_stdin = attached.stdin().context("no stdin on exec session")?;

    // Deliberately not `tokio::io::copy`: it only flushes the writer once
    // the *source* hits EOF, which an interactive session never does until
    // the whole thing ends — so every byte of remote output would sit
    // buffered and invisible until exit. Flush after every read instead.
    let mut stdout_task = tokio::spawn(async move {
        let mut stdout = tokio::io::stdout();
        let mut buf = [0u8; 4096];
        loop {
            match remote_stdout.read(&mut buf).await {
                Ok(0) | Err(_) => break,
                Ok(n) => {
                    if stdout.write_all(&buf[..n]).await.is_err() || stdout.flush().await.is_err() {
                        break;
                    }
                }
            }
        }
    });
    let stdin_task = tokio::spawn(async move {
        let mut stdin = tokio::io::stdin();
        let mut buf = [0u8; 4096];
        loop {
            match stdin.read(&mut buf).await {
                Ok(0) | Err(_) => break,
                Ok(n) => {
                    if remote_stdin.write_all(&buf[..n]).await.is_err()
                        || remote_stdin.flush().await.is_err()
                    {
                        break;
                    }
                }
            }
        }
    });

    // `attached.join()` is meant to signal "the remote process is done",
    // but in practice it can hang well past that point (observed against a
    // real cluster: it never resolved after the remote shell had already
    // exited and closed its output). The stdout pump ending — remote
    // closed its output, i.e. the process is gone — is the signal we
    // actually care about, so race the two instead of trusting join()
    // alone; whichever concludes first ends the session.
    tokio::select! {
        _ = &mut stdout_task => {}
        _ = attached.join() => {}
    }
    stdout_task.abort();
    stdin_task.abort();
    Ok(())
}

/// Adds an ephemeral debug container to a running pod (equivalent to
/// `kubectl debug`), returning the name assigned so the caller can exec into it.
pub async fn add_debug_container(
    client: &Client,
    namespace: &str,
    pod: &str,
    image: &str,
    target_container: Option<&str>,
) -> Result<String> {
    let api: Api<Pod> = Api::namespaced(client.clone(), namespace);
    let name = format!("debug-{}", debug_container_suffix());

    let ephemeral = EphemeralContainer {
        name: name.clone(),
        image: Some(image.to_string()),
        stdin: Some(true),
        tty: Some(true),
        target_container_name: target_container.map(|s| s.to_string()),
        ..Default::default()
    };

    let patch = serde_json::json!({
        "spec": { "ephemeralContainers": [ephemeral] }
    });

    api.patch_ephemeral_containers(pod, &PatchParams::default(), &Patch::Strategic(&patch))
        .await
        .context("adding ephemeral debug container")?;

    wait_until_running(&api, pod, &name).await?;

    Ok(name)
}

/// The apiserver accepts an exec request for a container the instant it
/// exists in the spec, but actually attaching fails with a 500 until the
/// kubelet has pulled the image and started it — so poll status until the
/// named (ephemeral) container reports `running`, instead of racing it.
async fn wait_until_running(api: &Api<Pod>, pod: &str, container_name: &str) -> Result<()> {
    let deadline = tokio::time::Instant::now() + std::time::Duration::from_secs(60);
    loop {
        let current = api
            .get(pod)
            .await
            .context("polling debug container status")?;
        let running = current
            .status
            .as_ref()
            .and_then(|s| s.ephemeral_container_statuses.as_ref())
            .into_iter()
            .flatten()
            .any(|c| {
                c.name == container_name && c.state.as_ref().is_some_and(|s| s.running.is_some())
            });
        if running {
            return Ok(());
        }
        anyhow::ensure!(
            tokio::time::Instant::now() < deadline,
            "debug container '{container_name}' did not start within 60s"
        );
        tokio::time::sleep(std::time::Duration::from_millis(500)).await;
    }
}

/// A short, good-enough-to-avoid-collisions suffix for debug container names.
fn debug_container_suffix() -> String {
    use std::time::{SystemTime, UNIX_EPOCH};
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.subsec_nanos())
        .unwrap_or(0);
    format!("{nanos:x}")
}
