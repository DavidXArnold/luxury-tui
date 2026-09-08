# Feature parity with Luxury Yacht

Tracks Luxury TUI's implementation against [Luxury Yacht](https://github.com/luxury-yacht/app)
by John Jeffers. Luxury Yacht is a Go/GTK4 desktop GUI; Luxury TUI is a Rust
terminal app, so this is a reimplementation of the *ideas*, not a port of
any code — nothing here is copied from upstream's source.

**Currently tracking upstream:** `v2.2.1` (see `.github/upstream-version.txt`,
kept in sync by `.github/workflows/upstream-watch.yml`).

Every feature below is implemented except the two marked ➖, which don't
translate to a terminal grid at all (see the legend).

## Status

| Feature | Upstream | Luxury TUI | Notes |
|---|---|---|---|
| Zero-config kubeconfig detection | ✅ | ✅ | `k8s::context` — reads `$KUBECONFIG` / `~/.kube/config` |
| Cluster/context switching | ✅ | ✅ | context picker screen |
| Cluster overview dashboard | ✅ | ✅ | node/pod counts, warning events |
| Namespace filter | ✅ | ✅ | `n` cycles all -> ns1 -> ns2 -> ... |
| Workload browser (Deploy/STS/DS/RS/Job/CronJob/Svc) | ✅ | ✅ | Workloads screen |
| Node list + roles/version | ✅ | ✅ | Workloads -> Nodes |
| Split-pane workloads+pods | ✅ | ✅ | kind list + item list panes |
| Attention view (unhealthy pods) | ✅ | ✅ | Attention screen, `k8s::resources::filter_attention` |
| Advanced log viewer (follow, search) | ✅ | ✅ | Logs screen; regex highlighting not yet, plain substring search only |
| Log timestamps / JSON formatting | ✅ | ✅ | `T` toggles timestamps, `j` toggles a JSON-as-logfmt view (`ui::logs`) |
| Object maps (relationship tree) | ✅ | ✅ | `k8s::objectmap`, ASCII tree render |
| Object comparison / diff | ✅ | ✅ | `k8s::diff::compare_resources` — any one kind vs. itself (Pod, Deployment, StatefulSet, DaemonSet, ReplicaSet, Job, CronJob, Service, Node); `v` twice on same-kind items |
| Command palette | ✅ | ✅ | `:`, fuzzy-matched actions |
| Per-cluster theme colors | ✅ | ✅ | `t` cycles palette, persisted in config |
| Port forwarding | ✅ | ✅ | `p` on a pod, picks first container port |
| Shell access | ✅ | ✅ | `s` on a pod, suspends the TUI for an interactive shell |
| Debug container support | ✅ | ✅ | `S` on a pod prompts for an image, launches an ephemeral debug container, attaches a shell |
| Node cordon/drain/delete | ✅ | ✅ | `c` / `d` / `D` on Workloads -> Nodes |
| Multiple windows / draggable panels | ✅ | ➖ | not meaningful in a terminal; tabs + fixed split-panes are the TUI equivalent |
| Flexible/customizable layout | ✅ | ➖ | same as above — out of scope for a terminal grid |

Legend: ✅ done · ⏳ planned/partial · ➖ won't-port (doesn't translate to a TUI)

## Testing approach

Upstream's CI (`.github/workflows/release.yml` in their repo) is built
around Wails desktop packaging: Go `vet`/tests, a JS frontend
lint/typecheck/test pass, then signed & notarized installers per platform.
None of that applies to a single Rust binary, so ours is different in kind,
not just detail:

- **`cargo fmt --check`** and **`cargo clippy -D warnings`** on every push/PR.
- **`cargo test`** on Linux and macOS (see `src/**/tests` — pure-logic units:
  theme assignment, config persistence, attention filtering, object-map
  rendering, diffing, command palette matching).
- **`cargo build --release`** across our target matrix (linux x86_64/aarch64,
  macOS x86_64/aarch64) as a build-health check, gated behind fmt/clippy/test.
- **`cargo test --test cluster_integration`** (job `integration`) against a
  real [`kind`](https://kind.sigs.k8s.io/) cluster with
  [`kwok`](https://kwok.sigs.k8s.io/)'s controller installed alongside the
  real one — real pod scheduling for lifecycle/attention checks, cheap fake
  nodes (no real hardware) for cordon/drain checks. See
  `tests/cluster_integration.rs`; skipped locally unless
  `LUXURY_TUI_INTEGRATION=1` is set and a cluster is reachable, so it never
  affects the plain `cargo test` job. Kept as a separate, non-blocking
  status check rather than a `build` prerequisite since a live cluster is
  slower and flakier than the rest of the pipeline.

## Keeping up with upstream

`.github/workflows/upstream-watch.yml` runs weekly (and on demand), checks
upstream's latest GitHub release, and files a tracking issue when it's new
— nothing gets auto-ported (different language, different UI paradigm), but
it puts "go read the changelog" on a human's plate instead of relying on
someone remembering to check.
