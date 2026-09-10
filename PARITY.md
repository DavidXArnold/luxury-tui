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
| Zero-config kubeconfig detection | ✅ | ✅ | `k8s::context::discover` — scans `~/.kube` for any valid kubeconfig file (not just one named `config`), same directory-scan + per-file/per-context listing as upstream; `$KUBECONFIG` still honored |
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

## Manual verification against a real cluster

Automated tests don't cover everything — interactive exec/attach behavior
(raw-mode handling, echo, output flushing, keystroke routing during a shell
session) isn't meaningfully unit-testable and CI's kind+kwok job doesn't
drive the actual TUI, only the library functions underneath it. Those paths
were exercised directly against a real cluster (via `tmux send-keys` +
`capture-pane`, driving the compiled binary as a real interactive program),
which found and fixed several real bugs the automated suite couldn't have
caught:

- **Selection followed list index, not item identity.** A refresh could
  return the same nodes/pods in a different order (observed live), silently
  moving the highlight onto a different resource than the one selected —
  dangerous for `d`/`D` (drain/delete). Fixed by tracking selection as a
  `(kind, namespace, name)` identity (`app::ItemRef`) resolved against the
  current list every render (`App::resolve_selection`), covered by
  regression tests in `src/app.rs`.
- **Remote shell output never appeared.** `run_interactive_shell` used
  `tokio::io::copy`, which only flushes its writer when the *source* hits
  EOF — never true for a live interactive stream — so command output sat
  buffered and invisible until the session ended. Fixed with an explicit
  read/write/flush loop.
- **Double echo and broken control keys in an attached shell.** The local
  terminal was dropped out of raw mode for the shell session, so the local
  tty's own canonical-mode echo doubled up with the remote pty's echo, and
  Ctrl-C/Ctrl-D stopped behaving as raw bytes. Fixed by keeping raw mode on
  throughout — only the alternate screen is left/re-entered.
- **Exiting an attached shell could hang the whole app.** `attached.join()`
  was observed to never resolve after the remote process had already exited
  and closed its output. Fixed by racing it against the stdout pump ending
  (a closed remote stdout is the actually-reliable "the process is gone"
  signal).
- **Keystrokes leaked between an attached shell and the TUI.** The
  always-running crossterm input reader and the shell's own raw stdin pump
  both read the same fd at once, splitting keystrokes unpredictably and
  queuing whatever the TUI reader grabbed to fire the instant control
  returned — capable of misdirecting to destructive keys (`d`/`D`, `q`).
  Fixed by stopping the crossterm reader before an exec session starts and
  restarting it after.
- **Debug container exec raced the container starting.** Execing
  immediately after the ephemeral-container patch succeeded could hit the
  container before the kubelet had actually started it (`500` from the
  apiserver). Fixed by polling pod status for `running` first.
- Two cosmetic layout bugs: the splash logo overflowing its fixed-height
  layout region and colliding with the box below it (now sized from the
  logo text itself), and a namespace-filter label truncating in a
  20-column-wide panel (moved to the top bar, which has room).

## Keeping up with upstream

`.github/workflows/upstream-watch.yml` runs weekly (and on demand), checks
upstream's latest GitHub release, and files a tracking issue when it's new
— nothing gets auto-ported (different language, different UI paradigm), but
it puts "go read the changelog" on a human's plate instead of relying on
someone remembering to check.
