mod analytics;
mod model;

use std::{
    net::SocketAddr,
    path::{Path, PathBuf},
    process::Stdio,
    sync::Arc,
    time::{SystemTime, UNIX_EPOCH},
};

use anyhow::{Context, Result};
use axum::{
    extract::State,
    response::{
        sse::{Event, KeepAlive},
        Html, IntoResponse, Sse,
    },
    routing::get,
    Json, Router,
};
use clap::Parser;
use model::{Benchmark, CargoCriterionMessage, DashboardState, Run};
use tokio::{
    io::{AsyncBufReadExt, BufReader},
    process::Command,
    sync::{broadcast, RwLock},
};
use tokio_stream::{wrappers::BroadcastStream, StreamExt};

const INDEX_HTML: &str = include_str!("../assets/index.html");

#[derive(Debug, Parser)]
#[command(name = "cargo-critdash", bin_name = "cargo critdash")]
#[command(about = "Serve a live empirical dashboard while Criterion benchmarks run")]
struct Args {
    #[arg(long, default_value = "127.0.0.1")]
    host: String,

    #[arg(long, default_value_t = 8787)]
    port: u16,

    /// Label for this benchmark run. Defaults to the git revision.
    #[arg(long)]
    label: Option<String>,

    #[arg(long)]
    no_open: bool,

    /// Only serve previously recorded runs; do not start cargo-criterion.
    #[arg(long)]
    serve_only: bool,

    /// Exit when the benchmark process finishes instead of continuing to serve.
    #[arg(long)]
    exit_after_run: bool,

    #[arg(long, default_value = "target/critdash")]
    data_dir: PathBuf,

    /// Arguments passed through to `cargo criterion` after `--`.
    #[arg(last = true)]
    criterion_args: Vec<String>,
}

#[derive(Clone)]
struct AppState {
    dashboard: Arc<RwLock<DashboardState>>,
    updates: broadcast::Sender<()>,
}

#[tokio::main]
async fn main() -> Result<()> {
    let mut raw_args: Vec<String> = std::env::args().collect();
    // Cargo invokes subcommands as `cargo-critdash critdash ...`.
    if raw_args.get(1).is_some_and(|value| value == "critdash") {
        raw_args.remove(1);
    }
    let args = Args::parse_from(raw_args);

    let dashboard = load_history(&args.data_dir).await?;
    let (updates, _) = broadcast::channel(64);
    let state = AppState {
        dashboard: Arc::new(RwLock::new(dashboard)),
        updates,
    };

    let addr: SocketAddr = format!("{}:{}", args.host, args.port)
        .parse()
        .context("invalid --host/--port")?;
    let listener = tokio::net::TcpListener::bind(addr)
        .await
        .with_context(|| format!("failed to bind http://{addr}"))?;
    let app = Router::new()
        .route("/", get(index))
        .route("/api/state", get(api_state))
        .route("/events", get(events))
        .with_state(state.clone());

    println!("critdash: http://{addr}");
    if !args.no_open {
        open_browser(&format!("http://{addr}"));
    }

    let server = tokio::spawn(async move {
        if let Err(error) = axum::serve(listener, app).await {
            eprintln!("critdash server error: {error}");
        }
    });

    if !args.serve_only {
        let run = new_run(args.label.as_deref()).await;
        let run_id = run.id.clone();
        {
            let mut dashboard = state.dashboard.write().await;
            dashboard.runs.push(run);
        }
        persist_run(&state, &args.data_dir, &run_id).await?;
        notify(&state);

        let result = run_criterion(&state, &args.data_dir, &run_id, &args.criterion_args).await;
        {
            let mut dashboard = state.dashboard.write().await;
            if let Some(run) = dashboard.runs.iter_mut().find(|run| run.id == run_id) {
                run.complete = result.is_ok();
            }
        }
        persist_run(&state, &args.data_dir, &run_id).await?;
        notify(&state);
        result?;

        if args.exit_after_run {
            server.abort();
            return Ok(());
        }
    }

    println!("critdash: serving; press Ctrl-C to stop");
    tokio::signal::ctrl_c().await?;
    server.abort();
    Ok(())
}

async fn index() -> Html<&'static str> {
    Html(INDEX_HTML)
}

async fn api_state(State(state): State<AppState>) -> Json<DashboardState> {
    Json(state.dashboard.read().await.clone())
}

async fn events(State(state): State<AppState>) -> impl IntoResponse {
    let stream =
        BroadcastStream::new(state.updates.subscribe()).filter_map(|message| match message {
            Ok(()) => Some(Ok::<Event, std::convert::Infallible>(
                Event::default().data("state"),
            )),
            Err(_) => None,
        });
    Sse::new(stream).keep_alive(KeepAlive::default())
}

async fn run_criterion(
    state: &AppState,
    data_dir: &Path,
    run_id: &str,
    passthrough: &[String],
) -> Result<()> {
    anyhow::ensure!(
        command_output("cargo", &["criterion", "--version"]) != "unavailable",
        "cargo-criterion is required; install it with `cargo install cargo-criterion`"
    );

    let mut command = Command::new("cargo");
    command
        .arg("criterion")
        .arg("--message-format=json")
        .args(passthrough)
        .stdout(Stdio::piped())
        .stderr(Stdio::inherit());

    println!(
        "critdash: running cargo criterion --message-format=json {}",
        passthrough.join(" ")
    );
    let mut child = command
        .spawn()
        .context("failed to launch `cargo criterion`")?;
    let stdout = child
        .stdout
        .take()
        .context("cargo-criterion stdout unavailable")?;
    let mut lines = BufReader::new(stdout).lines();

    while let Some(line) = lines.next_line().await? {
        match serde_json::from_str::<CargoCriterionMessage>(&line) {
            Ok(message) if message.reason == "benchmark-complete" => {
                let benchmark = Benchmark::from_message(message)?;
                println!("critdash: received {}", benchmark.id);
                {
                    let mut dashboard = state.dashboard.write().await;
                    let run = dashboard
                        .runs
                        .iter_mut()
                        .find(|run| run.id == run_id)
                        .context("active run disappeared")?;
                    run.benchmarks.insert(benchmark.id.clone(), benchmark);
                }
                persist_run(state, data_dir, run_id).await?;
                notify(state);
            }
            Ok(_) => {}
            Err(_) => println!("{line}"),
        }
    }

    let status = child.wait().await?;
    anyhow::ensure!(status.success(), "cargo criterion exited with {status}");
    Ok(())
}

async fn new_run(label: Option<&str>) -> Run {
    let started_at_unix_ms = now_ms();
    let revision = command_output("git", &["rev-parse", "--short", "HEAD"]);
    let project = std::env::current_dir()
        .ok()
        .and_then(|path| {
            path.file_name()
                .map(|name| name.to_string_lossy().into_owned())
        })
        .unwrap_or_else(|| "project".into());
    let default_label = if revision != "unavailable" {
        revision.clone()
    } else {
        started_at_unix_ms.to_string()
    };

    Run {
        id: format!("{started_at_unix_ms}-{default_label}"),
        label: label.unwrap_or(&default_label).to_owned(),
        started_at_unix_ms,
        project,
        revision,
        machine: machine_description(),
        rustc: command_output("rustc", &["--version"]),
        cargo_criterion: command_output("cargo", &["criterion", "--version"]),
        complete: false,
        benchmarks: Default::default(),
    }
}

fn machine_description() -> String {
    let cpu = std::fs::read_to_string("/proc/cpuinfo")
        .ok()
        .and_then(|text| {
            text.lines()
                .find_map(|line| line.strip_prefix("model name\t: ").map(str::to_owned))
        })
        .unwrap_or_else(|| "unknown cpu".into());
    format!(
        "{} {} · {cpu}",
        std::env::consts::OS,
        std::env::consts::ARCH
    )
}

fn command_output(program: &str, args: &[&str]) -> String {
    std::process::Command::new(program)
        .args(args)
        .output()
        .ok()
        .filter(|output| output.status.success())
        .and_then(|output| String::from_utf8(output.stdout).ok())
        .map(|value| value.trim().to_owned())
        .filter(|value| !value.is_empty())
        .unwrap_or_else(|| "unavailable".into())
}

fn notify(state: &AppState) {
    let _ = state.updates.send(());
}

async fn load_history(data_dir: &Path) -> Result<DashboardState> {
    let run_dir = data_dir.join("runs");
    tokio::fs::create_dir_all(&run_dir).await?;
    let mut entries = tokio::fs::read_dir(&run_dir).await?;
    let mut runs = Vec::new();
    while let Some(entry) = entries.next_entry().await? {
        if entry.path().extension().and_then(|value| value.to_str()) != Some("json") {
            continue;
        }
        let bytes = tokio::fs::read(entry.path()).await?;
        match serde_json::from_slice::<Run>(&bytes) {
            Ok(run) => runs.push(run),
            Err(error) => eprintln!("critdash: ignoring {}: {error}", entry.path().display()),
        }
    }
    runs.sort_by_key(|run| run.started_at_unix_ms);
    Ok(DashboardState {
        schema_version: 1,
        runs,
    })
}

async fn persist_run(state: &AppState, data_dir: &Path, run_id: &str) -> Result<()> {
    let run = {
        let dashboard = state.dashboard.read().await;
        dashboard
            .runs
            .iter()
            .find(|run| run.id == run_id)
            .cloned()
            .context("run missing while persisting")?
    };
    let run_dir = data_dir.join("runs");
    tokio::fs::create_dir_all(&run_dir).await?;
    let path = run_dir.join(format!("{}.json", sanitize(run_id)));
    let temporary = path.with_extension("json.tmp");
    tokio::fs::write(&temporary, serde_json::to_vec_pretty(&run)?).await?;
    tokio::fs::rename(&temporary, &path).await?;
    Ok(())
}

fn sanitize(value: &str) -> String {
    value
        .chars()
        .map(|ch| {
            if ch.is_ascii_alphanumeric() || matches!(ch, '-' | '_' | '.') {
                ch
            } else {
                '_'
            }
        })
        .collect()
}

fn now_ms() -> u128 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("system clock before unix epoch")
        .as_millis()
}

fn open_browser(url: &str) {
    #[cfg(target_os = "linux")]
    let command = ("xdg-open", vec![url]);
    #[cfg(target_os = "macos")]
    let command = ("open", vec![url]);
    #[cfg(target_os = "windows")]
    let command = ("cmd", vec!["/C", "start", "", url]);

    #[cfg(any(target_os = "linux", target_os = "macos", target_os = "windows"))]
    {
        let _ = std::process::Command::new(command.0)
            .args(command.1)
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn();
    }
}
