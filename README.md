<p align="center"><img src="docs/logo.png" alt="Luxury TUI badge" width="260"></p>

<h1 align="center">Luxury TUI</h1>

> [!IMPORTANT]
> **Luxury TUI is an independent, unaffiliated fan project.** It is not made
> by, endorsed by, or associated with John Jeffers or Luxury Yacht in any
> way. The badge above is a from-scratch redraw modeled on Luxury Yacht's
> logo as a visual tribute — it is not upstream's actual artwork, and no
> upstream code, assets, or trademarks are used. See [Credit](#credit)
> below.

A fast, terminal-native Kubernetes cluster manager for Linux and macOS,
written in Rust. Luxury TUI is a terminal companion to
**[Luxury Yacht](https://github.com/luxury-yacht/app)** — same idea, same
workflows, reimplemented for the terminal and built for speed.

## Credit

Luxury TUI exists because of **[John Jeffers](https://johnjeffers.com/)**,
who designed and built [Luxury Yacht](https://github.com/luxury-yacht/app),
the GTK4 desktop app this project takes all of its ideas, feature set, and
UX vocabulary from — the cluster overview, object maps, attention view,
log viewer, per-cluster theming, object comparison, and more are all his
design. Luxury TUI is an independent, from-scratch Rust/terminal
reimplementation — it shares no code with upstream, is not affiliated with
or endorsed by John Jeffers or Luxury Yacht, and was written because a
terminal-first, keyboard-driven version felt worth building. If you like
this, **go star and use [Luxury Yacht](https://github.com/luxury-yacht/app)**;
it's the original, it has a GUI, and John built it.

See [PARITY.md](PARITY.md) for a feature-by-feature comparison and how this
project tracks upstream releases.

## Splash screen

The terminal app itself only ever renders ASCII (no image can display
reliably inside a terminal), shown on the context-picker screen at startup:

```
                      _.-'''''-._
                   .-'           '-.
                  /      .-'''-.     \
                 |      (   @   )     |
                  \      '-...-'     /
                   '-._         _.-'
                   .-'` ~  ~  ~ `'-.
                .'                    '.
               /                        \
              |                          |
              |         L u x u r y      |
              |                          |
              |          T u i           |
              |                          |
               \                        /
                '.                    .'
                 '-._              _.-'
                 .-'`              `'-.
                |   LUXURY-TUI  APP    |
                 '--------------------'
```

## Features

- **Zero-config setup** — auto-detects `$KUBECONFIG` / `~/.kube/config`,
  lists every context, connects on selection.
- **Cluster overview** — node/pod health at a glance, recent warning events.
- **Workload browser** — Pods, Deployments, StatefulSets, DaemonSets,
  ReplicaSets, Jobs, CronJobs, Services, and Nodes, filterable by namespace.
- **Attention view** — pods that are unhealthy, not fully ready, or
  restart-looping, surfaced without hunting for them.
- **Log viewer** — follow/pause, scroll back, live substring search with
  highlighting.
- **Object maps** — ASCII relationship tree from a pod up through its
  owning ReplicaSet/Deployment/etc. and out to the Services selecting it.
- **Object comparison** — diff two pods' specs side by side.
- **Command palette** (`:`) — fuzzy-matched navigation and actions.
- **Per-cluster color themes** — assigned automatically, cycle with `t`,
  remembered across runs.
- **Port forwarding** (`p`) and **interactive shell access** (`s`) into
  any pod, plus **node cordon/drain/delete**.

Full status against upstream, including what's intentionally out of scope
for a terminal UI (multi-window layouts, draggable panels), is in
[PARITY.md](PARITY.md).

## Install / Build

Requires a recent stable Rust toolchain ([rustup.rs](https://rustup.rs)).

```sh
git clone https://github.com/DavidXArnold/luxury-tui.git
cd luxury-tui
cargo build --release
./target/release/luxury-tui
```

Or grab a prebuilt binary from the [Releases](../../releases) page
(Linux x86_64/aarch64, macOS x86_64/aarch64).

## Usage

Launch `luxury-tui`, pick a context, and go. Press `?` any time for the
full keybinding reference. Highlights:

| Key | Action |
|---|---|
| `Tab` / `Shift+Tab` | cycle screens |
| `1`-`6` | jump to a screen |
| `n` | cycle namespace filter |
| `t` | cycle this cluster's color theme |
| `Enter` | view logs for the selected pod |
| `m` | map object relationships for the selected pod |
| `v` | mark an item for comparison; press again on a same-kind item to diff |
| `s` | open an interactive shell in the selected pod |
| `S` | launch a debug container in the selected pod, then shell into it |
| `p` | port-forward to the selected pod |
| `T` / `j` | toggle log timestamps / JSON-as-logfmt view (Logs screen) |
| `c` / `d` / `D` | cordon/uncordon, drain, delete a node |
| `:` | command palette |
| `q` | quit |

## Why a TUI?

Kubernetes work happens on servers you SSH into, over links you don't
always trust with a GUI's bandwidth. A terminal-native tool starts
instantly, runs anywhere a shell does, and never leaves the keyboard.
Luxury Yacht already got the workflow right — Luxury TUI just brings it
to the terminal, in Rust, with an eye on doing as little work as possible
per frame.

## License

GPL-3.0-or-later — see [LICENSE](LICENSE). Chosen to match the spirit of
[upstream Luxury Yacht](https://github.com/luxury-yacht/app), which is
also GPL-3.0.

## Contributing

Issues and PRs welcome. `cargo fmt`, `cargo clippy --all-targets -- -D
warnings`, and `cargo test` all run in CI and are expected to pass. See
[PARITY.md](PARITY.md) before starting work on a new feature — it tracks
what's implemented, what's planned, and what upstream has shipped that we
haven't looked at yet.

CI also runs `tests/cluster_integration.rs` against a real
[`kind`](https://kind.sigs.k8s.io/) cluster with
[`kwok`](https://kwok.sigs.k8s.io/) installed. To run those locally against
your own kind+kwok cluster:

```sh
LUXURY_TUI_INTEGRATION=1 KUBECONFIG=... cargo test --test cluster_integration
```

Without `LUXURY_TUI_INTEGRATION=1` set, those tests skip themselves — plain
`cargo test` never needs a cluster.
