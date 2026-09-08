//! Shell access into a running container, including spinning up an
//! ephemeral debug container first (Luxury Yacht's "debug container
//! support").

use anyhow::{Context as _, Result};
use k8s_openapi::api::core::v1::{EphemeralContainer, Pod};
use kube::api::{AttachParams, Patch, PatchParams};
use kube::{Api, Client};

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

    // Two plain byte-copy pumps: remote output -> our stdout, our stdin ->
    // remote input. Whichever side closes first ends its own task; the
    // other is cleaned up once `attached.join()` returns below.
    let stdout_task = tokio::spawn(async move {
        let mut stdout = tokio::io::stdout();
        let _ = tokio::io::copy(&mut remote_stdout, &mut stdout).await;
    });
    let stdin_task = tokio::spawn(async move {
        let mut stdin = tokio::io::stdin();
        let _ = tokio::io::copy(&mut stdin, &mut remote_stdin).await;
    });

    let _ = attached.join().await;
    stdout_task.abort();
    stdin_task.abort();
    Ok(())
}

/// Adds an ephemeral debug container to a running pod (equivalent to
/// `kubectl debug`), returning the name assigned so the caller can exec into it.
///
/// Not yet bound to a keybinding — needs an image-picker prompt in the UI
/// first (see README's feature status table).
#[allow(dead_code)]
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

    Ok(name)
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
