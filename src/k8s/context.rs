//! Kubeconfig discovery: "zero-configuration setup that auto-detects
//! kubeconfig files", same spirit as Luxury Yacht.

use anyhow::{Context as _, Result};
use kube::config::{KubeConfigOptions, Kubeconfig};
use kube::{Client, Config};

#[derive(Debug, Clone)]
pub struct KubeContext {
    pub name: String,
    pub cluster: String,
    pub user: String,
    pub namespace: Option<String>,
    pub is_current: bool,
}

/// Load the kubeconfig from an explicit path, `$KUBECONFIG`, or `~/.kube/config`.
pub fn load_kubeconfig(explicit: Option<&std::path::Path>) -> Result<Kubeconfig> {
    if let Some(path) = explicit {
        return Kubeconfig::read_from(path)
            .with_context(|| format!("reading kubeconfig at {}", path.display()));
    }
    Kubeconfig::read().context("reading kubeconfig (checked $KUBECONFIG and ~/.kube/config)")
}

pub fn list_contexts(kubeconfig: &Kubeconfig) -> Vec<KubeContext> {
    let current = kubeconfig.current_context.clone();
    kubeconfig
        .contexts
        .iter()
        .filter_map(|named| {
            let ctx = named.context.as_ref()?;
            Some(KubeContext {
                name: named.name.clone(),
                cluster: ctx.cluster.clone(),
                user: ctx.user.clone().unwrap_or_default(),
                namespace: ctx.namespace.clone(),
                is_current: current.as_deref() == Some(named.name.as_str()),
            })
        })
        .collect()
}

/// Build a client authenticated for a specific context in the kubeconfig.
pub async fn client_for_context(kubeconfig: &Kubeconfig, context_name: &str) -> Result<Client> {
    let options = KubeConfigOptions {
        context: Some(context_name.to_string()),
        cluster: None,
        user: None,
    };
    let config = Config::from_custom_kubeconfig(kubeconfig.clone(), &options)
        .await
        .with_context(|| format!("building client config for context '{context_name}'"))?;
    let client = Client::try_from(config).context("constructing kube client")?;
    Ok(client)
}
