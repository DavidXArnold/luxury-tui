//! Kubeconfig discovery.
//!
//! Follows the same approach as Luxury Yacht: rather than only looking for
//! a single file named `config`, this scans a configurable list of search
//! paths (default: the `~/.kube` directory) for *any* file that parses as a
//! valid kubeconfig, and surfaces one selectable entry per context per file
//! — so `~/.kube/config`, `~/.kube/prod-cluster.yaml`, `~/.kube/staging`,
//! etc. all show up without the user doing anything.
//!
//! `$KUBECONFIG` is still honored (a colon/semicolon-separated list of
//! specific files, the standard kubectl convention) and takes priority over
//! the directory scan when set, same as an explicit override from our own
//! config file.

use std::collections::HashMap;
use std::path::{Path, PathBuf};

use anyhow::{Context as _, Result};
use kube::config::{KubeConfigOptions, Kubeconfig};
use kube::{Client, Config};

#[derive(Debug, Clone)]
pub struct KubeContext {
    pub name: String,
    pub cluster: String,
    pub user: String,
    pub namespace: Option<String>,
    /// Matches this context's *own file's* `current-context`, not a
    /// global notion of "current" across every discovered file.
    pub is_current: bool,
    /// Which file this context came from — needed to build a client for
    /// it later (see `client_for_context`) and shown in the picker so
    /// same-named contexts from different files aren't ambiguous.
    pub source_path: PathBuf,
    pub source_display: String,
    pub is_default_file: bool,
    /// True if the context references a cluster or user name that isn't
    /// actually defined in the same file (a structural check only — no
    /// server connectivity attempted, same as Luxury Yacht's
    /// `ConfirmUsable` validation).
    pub invalid: bool,
    pub invalid_reason: Option<String>,
}

/// Every kubeconfig file discovered, keyed by its resolved path — kept
/// around so `client_for_context` can look up the *specific* file a
/// selected context came from without re-reading the disk.
pub type KubeconfigsByPath = HashMap<PathBuf, Kubeconfig>;

pub struct Discovered {
    pub contexts: Vec<KubeContext>,
    pub files: KubeconfigsByPath,
}

/// Resolves the list of paths to search: an explicit override if given,
/// else `$KUBECONFIG` (kubectl's own convention — specific files), else
/// Luxury Yacht's default of scanning the whole `~/.kube` directory.
pub fn search_paths(explicit: Option<&[PathBuf]>) -> Vec<PathBuf> {
    search_paths_given_env(explicit, std::env::var_os("KUBECONFIG"))
}

/// Same as `search_paths`, but takes the `$KUBECONFIG` value as a plain
/// argument instead of reading the environment — so tests can exercise
/// every branch without mutating global process state (which, on current
/// Rust, needs `unsafe` to do soundly).
fn search_paths_given_env(
    explicit: Option<&[PathBuf]>,
    kubeconfig_env: Option<std::ffi::OsString>,
) -> Vec<PathBuf> {
    if let Some(paths) = explicit {
        if !paths.is_empty() {
            return paths.to_vec();
        }
    }
    if let Some(from_env) = kubeconfig_env {
        let paths: Vec<PathBuf> = std::env::split_paths(&from_env)
            .filter(|p| !p.as_os_str().is_empty())
            .collect();
        if !paths.is_empty() {
            return paths;
        }
    }
    vec![PathBuf::from("~/.kube")]
}

/// Filters out obvious non-kubeconfig files during a directory scan —
/// ported from Luxury Yacht's `shouldSkipKubeconfigName`. Not applied to
/// paths given explicitly (a search path that's a file, not a directory):
/// naming it directly is enough intent.
fn should_skip_kubeconfig_name(name: &str) -> bool {
    if name.starts_with('.') && name != ".kubeconfig" {
        return true;
    }
    const SKIP_SUFFIXES: &[&str] = &[
        ".bak",
        ".backup",
        ".old",
        ".tmp",
        ".swp",
        ".swo",
        "~",
        ".orig",
        ".rej",
        ".lock",
        ".log",
        ".yaml.bak",
    ];
    let lower = name.to_lowercase();
    if SKIP_SUFFIXES.iter().any(|suffix| lower.ends_with(suffix)) {
        return true;
    }
    lower.contains("cache") || lower.contains("token") || lower.contains("credential")
}

/// Expands a leading `~` to the home directory, same as every shell does.
fn expand_home(path: &Path) -> PathBuf {
    let Ok(rest) = path.strip_prefix("~") else {
        return path.to_path_buf();
    };
    match directories::UserDirs::new() {
        Some(dirs) => dirs.home_dir().join(rest),
        None => path.to_path_buf(),
    }
}

fn default_kubeconfig_path() -> Option<PathBuf> {
    directories::UserDirs::new().map(|dirs| dirs.home_dir().join(".kube").join("config"))
}

/// Scans every search path (files are taken as-is; directories are scanned
/// non-recursively with `should_skip_kubeconfig_name` filtering), parses
/// each candidate, and keeps the ones that actually look like a kubeconfig
/// (at least one cluster and one context) — silently skipping anything
/// that isn't, exactly like Luxury Yacht does.
pub fn discover(explicit: Option<&[PathBuf]>) -> Discovered {
    let default_path = default_kubeconfig_path();
    let mut files = KubeconfigsByPath::new();
    let mut contexts = Vec::new();
    let mut seen = std::collections::HashSet::new();

    for entry in search_paths(explicit) {
        let resolved = expand_home(&entry);
        let Ok(metadata) = std::fs::metadata(&resolved) else {
            tracing::debug!("kubeconfig search path not found: {}", resolved.display());
            continue;
        };

        if metadata.is_dir() {
            let Ok(read_dir) = std::fs::read_dir(&resolved) else {
                continue;
            };
            for dir_entry in read_dir.flatten() {
                if dir_entry.file_type().map(|t| t.is_dir()).unwrap_or(false) {
                    continue;
                }
                let name = dir_entry.file_name();
                let name = name.to_string_lossy();
                if should_skip_kubeconfig_name(&name) {
                    continue;
                }
                try_add_kubeconfig(
                    dir_entry.path(),
                    &name,
                    default_path.as_deref(),
                    &mut seen,
                    &mut files,
                    &mut contexts,
                );
            }
        } else {
            let name = resolved
                .file_name()
                .map(|n| n.to_string_lossy().to_string())
                .unwrap_or_default();
            try_add_kubeconfig(
                resolved,
                &name,
                default_path.as_deref(),
                &mut seen,
                &mut files,
                &mut contexts,
            );
        }
    }

    Discovered { contexts, files }
}

fn try_add_kubeconfig(
    path: PathBuf,
    display_name: &str,
    default_path: Option<&Path>,
    seen: &mut std::collections::HashSet<PathBuf>,
    files: &mut KubeconfigsByPath,
    contexts: &mut Vec<KubeContext>,
) {
    let canonical = std::fs::canonicalize(&path).unwrap_or_else(|_| path.clone());
    if !seen.insert(canonical.clone()) {
        return;
    }

    let Ok(kubeconfig) = Kubeconfig::read_from(&path) else {
        return;
    };
    if kubeconfig.clusters.is_empty() || kubeconfig.contexts.is_empty() {
        return;
    }

    let is_default_file = default_path.is_some_and(|d| paths_equal(d, &path));
    let current = kubeconfig.current_context.clone();

    for named in &kubeconfig.contexts {
        let Some(ctx) = named.context.as_ref() else {
            continue;
        };
        let cluster_exists = kubeconfig.clusters.iter().any(|c| c.name == ctx.cluster);
        let user = ctx.user.clone().unwrap_or_default();
        let user_exists = user.is_empty() || kubeconfig.auth_infos.iter().any(|u| u.name == user);
        let (invalid, invalid_reason) = if !cluster_exists {
            (
                true,
                Some(format!("references undefined cluster '{}'", ctx.cluster)),
            )
        } else if !user_exists {
            (true, Some(format!("references undefined user '{user}'")))
        } else {
            (false, None)
        };

        contexts.push(KubeContext {
            name: named.name.clone(),
            cluster: ctx.cluster.clone(),
            user,
            namespace: ctx.namespace.clone(),
            is_current: current.as_deref() == Some(named.name.as_str()),
            source_path: canonical.clone(),
            source_display: display_name.to_string(),
            is_default_file,
            invalid,
            invalid_reason,
        });
    }

    files.insert(canonical, kubeconfig);
}

fn paths_equal(a: &Path, b: &Path) -> bool {
    let canon = |p: &Path| std::fs::canonicalize(p).unwrap_or_else(|_| p.to_path_buf());
    canon(a) == canon(b)
}

/// Build a client authenticated for a specific context in the kubeconfig
/// file it came from (see `KubeContext::source_path` / `Discovered::files`).
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn skips_dotfiles_except_dot_kubeconfig() {
        assert!(should_skip_kubeconfig_name(".DS_Store"));
        assert!(should_skip_kubeconfig_name(".git"));
        assert!(!should_skip_kubeconfig_name(".kubeconfig"));
    }

    #[test]
    fn skips_common_non_kubeconfig_suffixes() {
        for name in [
            "config.bak",
            "config.old",
            "config.tmp",
            "config.swp",
            "config~",
        ] {
            assert!(
                should_skip_kubeconfig_name(name),
                "expected {name} to be skipped"
            );
        }
    }

    #[test]
    fn skips_names_that_look_like_caches_or_credentials() {
        for name in ["http-cache.json", "gcloud-token.json", "aws-credentials"] {
            assert!(
                should_skip_kubeconfig_name(name),
                "expected {name} to be skipped"
            );
        }
    }

    #[test]
    fn keeps_ordinary_kubeconfig_looking_names() {
        for name in [
            "config",
            "prod-cluster.yaml",
            "staging",
            "my-kubeconfig.yml",
        ] {
            assert!(
                !should_skip_kubeconfig_name(name),
                "expected {name} to be kept"
            );
        }
    }

    #[test]
    fn kubeconfig_env_var_takes_priority_over_the_default_directory_scan() {
        let env_value =
            std::ffi::OsString::from(format!("/tmp/a/config{}/tmp/b/config", SEPARATOR));
        let paths = search_paths_given_env(None, Some(env_value));
        assert_eq!(
            paths,
            vec![
                PathBuf::from("/tmp/a/config"),
                PathBuf::from("/tmp/b/config")
            ]
        );
    }

    #[test]
    fn falls_back_to_scanning_the_kube_directory_when_nothing_else_is_set() {
        let paths = search_paths_given_env(None, None);
        assert_eq!(paths, vec![PathBuf::from("~/.kube")]);
    }

    #[test]
    fn an_explicit_override_beats_both_the_env_var_and_the_default() {
        let explicit = vec![PathBuf::from("/explicit/config")];
        let env_value = std::ffi::OsString::from("/tmp/a/config");
        let paths = search_paths_given_env(Some(&explicit), Some(env_value));
        assert_eq!(paths, explicit);
    }

    #[cfg(unix)]
    const SEPARATOR: char = ':';
    #[cfg(windows)]
    const SEPARATOR: char = ';';
}
