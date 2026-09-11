use luxury_tui::{app, config, event, k8s, logo, tasks, theme, ui};

use std::io::stdout;
use std::time::Duration;

use crossterm::event::{
    DisableMouseCapture, EnableMouseCapture, Event, KeyCode, KeyEventKind, KeyModifiers,
};
use crossterm::execute;
use crossterm::terminal::{
    disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen,
};
use ratatui::backend::CrosstermBackend;
use ratatui::Terminal;

use app::{App, Screen, WorkloadKind, WORKLOAD_KINDS};
use config::Config;
use event::{spawn_input_forwarder, spawn_ticker, AppEvent};
use theme::Theme;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    color_eyre::install().ok();
    let _log_guard = init_logging();

    let mut app = App::new(Config::load());

    enable_raw_mode()?;
    execute!(
        stdout(),
        EnterAlternateScreen,
        EnableMouseCapture,
        crossterm::terminal::SetTitle(logo::LOGO_SMALL)
    )?;
    let backend = CrosstermBackend::new(stdout());
    let mut terminal = Terminal::new(backend)?;

    let (tx, mut rx) = event::channel();
    let mut input_forwarder = spawn_input_forwarder(tx.clone());
    spawn_ticker(tx.clone(), Duration::from_millis(500));

    // Kick off kubeconfig discovery: scans ~/.kube (or $KUBECONFIG, or our
    // own config override) for every file that looks like a kubeconfig —
    // plain filesystem work, so run it on a blocking-friendly thread
    // rather than tying up an async worker.
    {
        let tx = tx.clone();
        let explicit = app.config.kubeconfig_search_paths.clone();
        tokio::task::spawn_blocking(move || {
            let explicit = (!explicit.is_empty()).then_some(explicit.as_slice());
            k8s::context::discover(explicit)
        })
        .await
        .map(|discovered| {
            let _ = tx.send(AppEvent::ContextsLoaded(Ok(discovered)));
        })
        .unwrap_or_else(|e| {
            let _ = tx.send(AppEvent::ContextsLoaded(Err(anyhow::anyhow!(e))));
        });
    }

    let result = run(&mut terminal, &mut app, tx, &mut rx, &mut input_forwarder).await;
    input_forwarder.abort();

    disable_raw_mode()?;
    execute!(
        terminal.backend_mut(),
        LeaveAlternateScreen,
        DisableMouseCapture
    )?;
    terminal.show_cursor()?;

    if let Err(e) = &result {
        eprintln!("luxury-tui exited with error: {e:#}");
    }
    result
}

fn init_logging() -> Option<tracing_appender::non_blocking::WorkerGuard> {
    let dir = directories::ProjectDirs::from("dev", "luxury-tui", "luxury-tui")?
        .data_dir()
        .to_path_buf();
    std::fs::create_dir_all(&dir).ok()?;
    let file_appender = tracing_appender::rolling::daily(dir, "luxury-tui.log");
    let (non_blocking, guard) = tracing_appender::non_blocking(file_appender);
    tracing_subscriber::fmt()
        .with_env_filter(tracing_subscriber::EnvFilter::from_default_env())
        .with_writer(non_blocking)
        .with_ansi(false)
        .init();
    Some(guard)
}

async fn run(
    terminal: &mut Terminal<CrosstermBackend<std::io::Stdout>>,
    app: &mut App,
    tx: event::EventSender,
    rx: &mut event::EventReceiver,
    input_forwarder: &mut tokio::task::JoinHandle<()>,
) -> anyhow::Result<()> {
    loop {
        terminal.draw(|f| ui::draw(f, app))?;

        let Some(evt) = rx.recv().await else { break };
        handle_event(app, &tx, evt).await;

        if let Some((namespace, pod, container)) = app.pending_shell.take() {
            // Stop the crossterm event reader before touching stdin
            // ourselves: it and our own raw read loop would otherwise both
            // be reading the same fd at once, splitting keystrokes between
            // "goes to the remote shell" and "queues up as a TUI command,
            // to fire the instant we get control back" pretty much at
            // random. Wait for it to actually stop (abort() only requests
            // cancellation) before proceeding.
            input_forwarder.abort();
            let _ = (&mut *input_forwarder).await;

            // Leave the alternate screen so the remote shell's own output
            // isn't fighting ratatui's, but keep raw mode ON throughout:
            // turning it off (as this used to) hands input to the local
            // tty's line-buffered/canonical mode, which echoes locally on
            // top of the remote pty's own echo — every line shows up
            // doubled and control keys stop working as expected. Real
            // terminal passthrough (ssh, `kubectl exec -it`) always keeps
            // raw mode on and lets the *remote* pty be the only thing
            // echoing.
            execute!(
                terminal.backend_mut(),
                LeaveAlternateScreen,
                DisableMouseCapture
            )
            .ok();
            if let Some(client) = app.client.clone() {
                let shell = pick_shell();
                if let Err(e) = k8s::exec::run_interactive_shell(
                    &client,
                    &namespace,
                    &pod,
                    container.as_deref(),
                    &shell,
                )
                .await
                {
                    app.set_status(format!("exec failed: {e:#}"));
                }
            }
            execute!(
                terminal.backend_mut(),
                EnterAlternateScreen,
                EnableMouseCapture
            )
            .ok();
            terminal.clear().ok();
            *input_forwarder = spawn_input_forwarder(tx.clone());
        }

        if app.should_quit {
            break;
        }
    }
    Ok(())
}

fn pick_shell() -> String {
    "/bin/sh".to_string()
}

async fn handle_event(app: &mut App, tx: &event::EventSender, evt: AppEvent) {
    match evt {
        AppEvent::Term(Event::Key(key)) if key.kind == KeyEventKind::Press => {
            handle_key(app, tx, key).await;
        }
        AppEvent::Term(_) => {}
        AppEvent::Tick => {}

        AppEvent::ContextsLoaded(Ok(discovered)) => {
            app.contexts = discovered.contexts;
            app.kubeconfig_files = discovered.files;
            let invalid = app.contexts.iter().filter(|c| c.invalid).count();
            let extra = if invalid > 0 {
                format!(" ({invalid} invalid)")
            } else {
                String::new()
            };
            app.set_status(format!(
                "found {} context(s) across {} file(s){extra}",
                app.contexts.len(),
                app.kubeconfig_files.len()
            ));
        }
        AppEvent::ContextsLoaded(Err(e)) => {
            app.set_status(format!("failed to load kubeconfig: {e:#}"));
        }
        AppEvent::ClientReady(name, client) => {
            app.client = Some(client.clone());
            app.current_context_name = Some(name.clone());
            app.theme = Theme::from_palette(app.config.palette_for(&name));
            app.screen = Screen::Overview;
            app.set_status(format!("connected to {name}"));
            refresh_all(app, tx);
        }
        AppEvent::ClientFailed(msg) => {
            app.set_status(format!("connection failed: {msg}"));
        }

        AppEvent::NamespacesLoaded(Ok(ns)) => app.namespaces = ns,
        AppEvent::NamespacesLoaded(Err(e)) => app.set_status(format!("namespaces: {e:#}")),
        AppEvent::PodsLoaded(Ok(pods)) => app.pods = pods,
        AppEvent::PodsLoaded(Err(e)) => app.set_status(format!("pods: {e:#}")),
        AppEvent::NodesLoaded(Ok(nodes)) => app.nodes = nodes,
        AppEvent::NodesLoaded(Err(e)) => app.set_status(format!("nodes: {e:#}")),
        AppEvent::WorkloadsLoaded(kind, Ok(items)) => {
            app.workloads_cache.insert(kind, items);
        }
        AppEvent::WorkloadsLoaded(kind, Err(e)) => app.set_status(format!("{kind}: {e:#}")),
        AppEvent::EventsLoaded(Ok(events)) => app.events = events,
        AppEvent::EventsLoaded(Err(e)) => app.set_status(format!("events: {e:#}")),

        AppEvent::LogLine(Ok(line)) => {
            app.logs.lines.push(line);
            if app.logs.lines.len() > 10_000 {
                let excess = app.logs.lines.len() - 10_000;
                app.logs.lines.drain(0..excess);
            }
        }
        AppEvent::LogLine(Err(e)) => app.set_status(format!("log stream: {e:#}")),
        AppEvent::LogStreamEnded => app.set_status("log stream ended"),

        AppEvent::ObjectMapLoaded(Ok(root)) => {
            app.object_map.rendered = k8s::objectmap::render_ascii(&root);
            app.object_map.root = Some(root);
        }
        AppEvent::ObjectMapLoaded(Err(e)) => app.set_status(format!("object map: {e:#}")),

        AppEvent::DiffReady(Ok(diff)) => app.compare.diff = diff,
        AppEvent::DiffReady(Err(e)) => app.set_status(format!("compare: {e:#}")),

        AppEvent::DrainFinished(Ok(outcome)) => {
            app.set_status(format!(
                "drained: {} evicted, {} daemonset-skipped, {} failed",
                outcome.evicted.len(),
                outcome.skipped_daemonset.len(),
                outcome.failed.len()
            ));
            if let Some(client) = app.client.clone() {
                tasks::refresh_nodes(client, tx.clone());
            }
        }
        AppEvent::DrainFinished(Err(e)) => app.set_status(format!("drain failed: {e:#}")),
        AppEvent::ActionResult(Ok(msg)) => {
            app.set_status(msg);
            if let Some(client) = app.client.clone() {
                tasks::refresh_nodes(client, tx.clone());
            }
        }
        AppEvent::ActionResult(Err(e)) => app.set_status(format!("action failed: {e:#}")),
        AppEvent::PortForwardStarted(Ok(desc)) => {
            app.set_status(format!("port-forward started: {desc}"));
            app.port_forwards.push(desc);
        }
        AppEvent::PortForwardStarted(Err(e)) => {
            app.set_status(format!("port-forward failed: {e:#}"))
        }
        AppEvent::DebugContainerReady(Ok((namespace, pod, container))) => {
            app.set_status(format!(
                "debug container {container} ready, attaching shell..."
            ));
            app.pending_shell = Some((namespace, pod, Some(container)));
        }
        AppEvent::DebugContainerReady(Err(e)) => {
            app.set_status(format!("debug container failed: {e:#}"))
        }
    }
}

/// Refreshes the baseline data every screen needs (Overview and Attention
/// both read pods/nodes/namespaces/events regardless of what's selected on
/// the Workloads screen) plus whichever *one* workload kind is currently
/// selected there.
///
/// Earlier this fired all 10 listers — pods, nodes, events, and all 7
/// other workload kinds — on every namespace change or 'r' press,
/// regardless of what was on screen. Harmless at small scale, but each of
/// those is a full, cluster-wide LIST call; at hundreds of nodes and
/// thousands of pods that's 7 needless full-cluster round trips (and 7x
/// the JSON to transfer and deserialize) every time, just because the user
/// might look at Deployments next. Now only the visible kind gets fetched;
/// switching kinds (see `cycle_kind`) fetches on demand instead.
fn refresh_all(app: &App, tx: &event::EventSender) {
    let Some(client) = app.client.clone() else {
        return;
    };
    let ns = app.namespace_filter.clone();
    tasks::refresh_namespaces(client.clone(), tx.clone());
    tasks::refresh_pods(client.clone(), ns.clone(), tx.clone());
    tasks::refresh_nodes(client.clone(), tx.clone());
    tasks::refresh_events(client.clone(), ns.clone(), tx.clone());
    refresh_current_kind(app, &client, ns, tx);
}

/// Fetches whichever single workload kind is currently selected, if it
/// isn't already covered by `refresh_all`'s baseline (Pods and Nodes are —
/// they're fetched unconditionally above).
fn refresh_current_kind(
    app: &App,
    client: &kube::Client,
    ns: Option<String>,
    tx: &event::EventSender,
) {
    let client = client.clone();
    match app.workload_kind {
        WorkloadKind::Pods | WorkloadKind::Nodes => {} // covered by the baseline fetch already
        WorkloadKind::Deployments => tasks::refresh_deployments(client, ns, tx.clone()),
        WorkloadKind::StatefulSets => tasks::refresh_statefulsets(client, ns, tx.clone()),
        WorkloadKind::DaemonSets => tasks::refresh_daemonsets(client, ns, tx.clone()),
        WorkloadKind::ReplicaSets => tasks::refresh_replicasets(client, ns, tx.clone()),
        WorkloadKind::Jobs => tasks::refresh_jobs(client, ns, tx.clone()),
        WorkloadKind::CronJobs => tasks::refresh_cronjobs(client, ns, tx.clone()),
        WorkloadKind::Services => tasks::refresh_services(client, ns, tx.clone()),
    }
}

async fn handle_key(app: &mut App, tx: &event::EventSender, key: crossterm::event::KeyEvent) {
    if app.palette.active {
        handle_palette_key(app, tx, key);
        return;
    }

    if app.debug_prompt.is_some() {
        handle_debug_prompt_key(app, tx, key);
        return;
    }

    if app.screen == Screen::ContextPicker {
        handle_context_picker_key(app, tx, key);
        return;
    }

    match key.code {
        KeyCode::Char('q') if key.modifiers.is_empty() => app.should_quit = true,
        KeyCode::Char('c') if key.modifiers.contains(KeyModifiers::CONTROL) => {
            app.should_quit = true
        }
        KeyCode::Char(':') => {
            app.palette.active = true;
            app.palette.query.clear();
            app.palette.selected = 0;
        }
        KeyCode::Char('?') => app.screen = Screen::Help,
        KeyCode::Esc if app.screen == Screen::Help => app.screen = Screen::Overview,
        KeyCode::Tab => app.next_screen(),
        KeyCode::BackTab => app.prev_screen(),
        KeyCode::Char('1') => app.screen = Screen::Overview,
        KeyCode::Char('2') => app.screen = Screen::Workloads,
        KeyCode::Char('3') => app.screen = Screen::Attention,
        KeyCode::Char('4') => app.screen = Screen::Logs,
        KeyCode::Char('5') => app.screen = Screen::ObjectMap,
        KeyCode::Char('6') => app.screen = Screen::Compare,
        KeyCode::Char('r') => refresh_all(app, tx),
        KeyCode::Char('n') => cycle_namespace(app, tx),
        KeyCode::Char('t') => cycle_theme(app),
        _ => match app.screen {
            Screen::Workloads | Screen::Attention => handle_workloads_key(app, tx, key),
            Screen::Logs => handle_logs_key(app, key),
            _ => {}
        },
    }
}

/// 't': cycle this cluster's assigned color palette and remember the choice,
/// same idea as Luxury Yacht's per-cluster theme colors.
fn cycle_theme(app: &mut App) {
    let Some(context) = app.current_context_name.clone() else {
        return;
    };
    let current = app.config.palette_for(&context);
    let idx = theme::Palette::ALL
        .iter()
        .position(|p| *p == current)
        .unwrap_or(0);
    let next = theme::Palette::ALL[(idx + 1) % theme::Palette::ALL.len()];
    app.config.set_palette(&context, next);
    app.theme = Theme::from_palette(next);
    if let Err(e) = app.config.save() {
        app.set_status(format!("theme set to {} (not saved: {e:#})", next.name()));
    } else {
        app.set_status(format!("theme set to {}", next.name()));
    }
}

fn cycle_namespace(app: &mut App, tx: &event::EventSender) {
    let mut options: Vec<Option<String>> = vec![None];
    options.extend(app.namespaces.iter().cloned().map(Some));
    let current_idx = options
        .iter()
        .position(|o| o == &app.namespace_filter)
        .unwrap_or(0);
    app.namespace_filter = options[(current_idx + 1) % options.len()].clone();
    refresh_all(app, tx);
}

fn handle_context_picker_key(
    app: &mut App,
    tx: &event::EventSender,
    key: crossterm::event::KeyEvent,
) {
    match key.code {
        KeyCode::Char('q') => app.should_quit = true,
        KeyCode::Up | KeyCode::Char('k') => {
            if app.context_selected > 0 {
                app.context_selected -= 1;
            }
        }
        KeyCode::Down | KeyCode::Char('j') => {
            if app.context_selected + 1 < app.contexts.len() {
                app.context_selected += 1;
            }
        }
        KeyCode::Enter => {
            if let Some(ctx) = app.contexts.get(app.context_selected).cloned() {
                if ctx.invalid {
                    app.set_status(format!(
                        "context '{}' looks broken: {}",
                        ctx.name,
                        ctx.invalid_reason.as_deref().unwrap_or("invalid")
                    ));
                    return;
                }
                let Some(kubeconfig) = app.kubeconfig_files.get(&ctx.source_path).cloned() else {
                    app.set_status("kubeconfig not loaded yet");
                    return;
                };
                let tx = tx.clone();
                let name = ctx.name.clone();
                tokio::spawn(async move {
                    match k8s::context::client_for_context(&kubeconfig, &name).await {
                        Ok(client) => {
                            let _ = tx.send(AppEvent::ClientReady(name, client));
                        }
                        Err(e) => {
                            let _ = tx.send(AppEvent::ClientFailed(e.to_string()));
                        }
                    }
                });
            }
        }
        _ => {}
    }
}

fn handle_palette_key(app: &mut App, tx: &event::EventSender, key: crossterm::event::KeyEvent) {
    match key.code {
        KeyCode::Esc => app.palette.active = false,
        KeyCode::Enter => {
            let results = ui::palette::matches(&app.palette.query);
            if let Some(action) = results.get(app.palette.selected) {
                run_palette_action(app, tx, action);
            }
            app.palette.active = false;
        }
        KeyCode::Up => app.palette.selected = app.palette.selected.saturating_sub(1),
        KeyCode::Down => app.palette.selected += 1,
        KeyCode::Backspace => {
            app.palette.query.pop();
            app.palette.selected = 0;
        }
        KeyCode::Char(c) => {
            app.palette.query.push(c);
            app.palette.selected = 0;
        }
        _ => {}
    }
}

fn run_palette_action(app: &mut App, tx: &event::EventSender, action: &str) {
    match action {
        "goto overview" => app.screen = Screen::Overview,
        "goto workloads" => app.screen = Screen::Workloads,
        "goto attention" => app.screen = Screen::Attention,
        "goto logs" => app.screen = Screen::Logs,
        "goto object-map" => app.screen = Screen::ObjectMap,
        "goto compare" => app.screen = Screen::Compare,
        "namespace all" => {
            app.namespace_filter = None;
            refresh_all(app, tx);
        }
        "refresh" => refresh_all(app, tx),
        "quit" => app.should_quit = true,
        _ => {}
    }
}

fn handle_workloads_key(app: &mut App, tx: &event::EventSender, key: crossterm::event::KeyEvent) {
    match key.code {
        KeyCode::Left => cycle_kind(app, tx, -1),
        KeyCode::Right => cycle_kind(app, tx, 1),
        KeyCode::Up | KeyCode::Char('k') => move_selection(app, -1),
        KeyCode::Down | KeyCode::Char('j') => move_selection(app, 1),
        KeyCode::Enter => {
            if app.workload_kind == WorkloadKind::Pods {
                if let Some(pod) = selected_pod(app) {
                    start_logs_for(app, tx, pod.namespace.clone(), pod.name.clone());
                }
            }
        }
        KeyCode::Char('m') => {
            if let (Some(pod), Some(client)) = (selected_pod(app), app.client.clone()) {
                tasks::load_object_map(client, pod.namespace.clone(), pod.name.clone(), tx.clone());
                app.screen = Screen::ObjectMap;
            }
        }
        KeyCode::Char('s') => {
            if let Some(pod) = selected_pod(app) {
                app.pending_shell = Some((pod.namespace.clone(), pod.name.clone(), None));
            }
        }
        KeyCode::Char('S') => {
            if let Some(pod) = selected_pod(app) {
                app.debug_prompt = Some(app::DebugContainerPrompt {
                    namespace: pod.namespace.clone(),
                    pod: pod.name.clone(),
                    image: "busybox:latest".to_string(),
                });
            }
        }
        KeyCode::Char('p') => {
            if let (Some(pod), Some(client)) = (selected_pod(app), app.client.clone()) {
                tasks::start_port_forward(
                    client,
                    pod.namespace.clone(),
                    pod.name.clone(),
                    tx.clone(),
                );
            }
        }
        KeyCode::Char('v') => mark_for_compare(app, tx),
        KeyCode::Char('c') if app.workload_kind == WorkloadKind::Nodes => {
            if let (Some(node), Some(client)) = (selected_node(app), app.client.clone()) {
                let unschedulable = node.schedulable;
                tasks::cordon_node(client, node.name.clone(), unschedulable, tx.clone());
            }
        }
        KeyCode::Char('d') if app.workload_kind == WorkloadKind::Nodes => {
            if let (Some(node), Some(client)) = (selected_node(app), app.client.clone()) {
                tasks::drain_node(client, node.name.clone(), tx.clone());
            }
        }
        KeyCode::Char('D') if app.workload_kind == WorkloadKind::Nodes => {
            if let (Some(node), Some(client)) = (selected_node(app), app.client.clone()) {
                tasks::delete_node(client, node.name.clone(), tx.clone());
            }
        }
        _ => {}
    }
}

fn cycle_kind(app: &mut App, tx: &event::EventSender, delta: i32) {
    let idx = WORKLOAD_KINDS
        .iter()
        .position(|k| *k == app.workload_kind)
        .unwrap_or(0) as i32;
    let len = WORKLOAD_KINDS.len() as i32;
    let next = ((idx + delta) % len + len) % len;
    app.workload_kind = WORKLOAD_KINDS[next as usize];
    // Let the next render pick index 0 of the new kind's list — trying to
    // guess an index here would just be trusting a number that's about to
    // mean something completely different anyway.
    app.selected = None;

    // Since refresh_all no longer fetches every kind up front (see its
    // doc comment), switching to a kind we haven't loaded yet needs its
    // own fetch — otherwise it'd just show whatever was cached from
    // whenever this kind was last selected, which might be stale or
    // (on first visit) empty.
    if let Some(client) = app.client.clone() {
        refresh_current_kind(app, &client, app.namespace_filter.clone(), tx);
    }
}

fn move_selection(app: &mut App, delta: i32) {
    let attention_only = app.screen == Screen::Attention;
    let refs = app.visible_item_refs(attention_only);
    if refs.is_empty() {
        return;
    }
    let current = app.resolve_selection(&refs).unwrap_or(0) as i32;
    let next = ((current + delta).max(0) as usize).min(refs.len() - 1);
    app.selected = Some(refs[next].clone());
}

/// 'v' on any workload list: first press marks the "left" side of a
/// comparison, second press (on another item of the *same kind*) marks the
/// "right" side and runs the diff. A third press starts a fresh comparison.
fn mark_for_compare(app: &mut App, tx: &event::EventSender) {
    let Some(picked) = selected_workload_ref(app) else {
        return;
    };

    if app.compare.left.is_none() {
        app.compare.left = Some(picked);
        app.set_status("marked for compare — pick a second, same-kind item and press 'v' again");
    } else if app.compare.right.is_none() {
        let left = app.compare.left.clone().unwrap();
        if left.kind != picked.kind {
            app.set_status(format!(
                "can't compare a {} with a {} — pick another {}",
                left.kind, picked.kind, left.kind
            ));
            return;
        }
        app.compare.right = Some(picked.clone());
        if let Some(client) = app.client.clone() {
            tasks::compare_resources(
                client,
                left.kind.clone(),
                (left.namespace.clone(), left.name.clone()),
                (picked.namespace.clone(), picked.name.clone()),
                tx.clone(),
            );
            app.screen = Screen::Compare;
        }
    } else {
        app.compare.left = Some(picked);
        app.compare.right = None;
        app.compare.diff.clear();
        app.set_status("marked for compare — pick a second, same-kind item and press 'v' again");
    }
}

/// The row currently selected on the Workloads/Attention screen, as a
/// compare target — works for any kind (Pods, Deployments, Nodes, ...).
/// `app.selected` is the identity resolved at the last render, so this is
/// safe to trust even if the underlying list has since been re-fetched.
fn selected_workload_ref(app: &App) -> Option<app::ItemRef> {
    app.selected.clone()
}

fn selected_pod(app: &App) -> Option<crate::k8s::resources::PodSummary> {
    let sel = app.selected.as_ref()?;
    if sel.kind != "Pod" {
        return None;
    }
    app.pods
        .iter()
        .find(|p| p.namespace == sel.namespace && p.name == sel.name)
        .cloned()
}

fn selected_node(app: &App) -> Option<crate::k8s::resources::NodeSummary> {
    let sel = app.selected.as_ref()?;
    if sel.kind != "Node" {
        return None;
    }
    app.nodes.iter().find(|n| n.name == sel.name).cloned()
}

fn start_logs_for(app: &mut App, tx: &event::EventSender, namespace: String, pod: String) {
    let Some(client) = app.client.clone() else {
        return;
    };
    app.logs.lines.clear();
    app.logs.target = Some((namespace.clone(), pod.clone(), None));
    app.logs.follow = true;
    app.screen = Screen::Logs;
    let req = k8s::logs::LogRequest {
        namespace,
        pod,
        container: None,
        follow: true,
        tail_lines: Some(500),
        // Always ask Kubernetes for timestamps; app.logs.show_timestamps
        // controls whether we display them, so toggling that never needs to
        // restart the stream.
        timestamps: true,
        previous: false,
    };
    tasks::start_log_stream(client, req, tx.clone());
}

fn handle_logs_key(app: &mut App, key: crossterm::event::KeyEvent) {
    // While actively typing a search term, every key is text input — that
    // way a search for e.g. "fail" doesn't also toggle follow via its 'f'.
    if app.logs.editing_search {
        match key.code {
            KeyCode::Enter | KeyCode::Esc => app.logs.editing_search = false,
            KeyCode::Backspace => {
                app.logs.search.pop();
            }
            KeyCode::Char(c) => app.logs.search.push(c),
            _ => {}
        }
        return;
    }

    match key.code {
        KeyCode::Char('/') => {
            app.logs.editing_search = true;
            app.logs.search.clear();
        }
        KeyCode::Char('f') => {
            app.logs.follow = !app.logs.follow;
            if app.logs.follow {
                app.logs.scroll = 0;
            }
        }
        KeyCode::Char('T') => app.logs.show_timestamps = !app.logs.show_timestamps,
        KeyCode::Char('j') => app.logs.json_pretty = !app.logs.json_pretty,
        KeyCode::Up => {
            app.logs.follow = false;
            app.logs.scroll = app.logs.scroll.saturating_add(1);
        }
        KeyCode::Down => {
            app.logs.scroll = app.logs.scroll.saturating_sub(1);
        }
        _ => {}
    }
}

fn handle_debug_prompt_key(
    app: &mut App,
    tx: &event::EventSender,
    key: crossterm::event::KeyEvent,
) {
    let Some(prompt) = app.debug_prompt.as_mut() else {
        return;
    };
    match key.code {
        KeyCode::Esc => app.debug_prompt = None,
        KeyCode::Enter => {
            let prompt = app.debug_prompt.take().unwrap();
            if let Some(client) = app.client.clone() {
                app.set_status(format!("launching debug container ({})...", prompt.image));
                tasks::add_debug_container(
                    client,
                    prompt.namespace,
                    prompt.pod,
                    prompt.image,
                    tx.clone(),
                );
            }
        }
        KeyCode::Backspace => {
            prompt.image.pop();
        }
        KeyCode::Char(c) => prompt.image.push(c),
        _ => {}
    }
}
