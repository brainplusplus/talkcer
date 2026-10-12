//! Talkcer CLI.
//!
//! Task 1 implements `talkcer-cli serve`, which starts the local Axum server.

use std::net::SocketAddr;
use std::path::PathBuf;

use anyhow::{bail, Context};

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new("info")),
        )
        .init();

    let args: Vec<String> = std::env::args().skip(1).collect();
    match args.first().map(String::as_str) {
        Some("serve") => serve(&args[1..]).await,
        Some(other) => bail!("unknown command `{other}`. available: serve"),
        None => bail!("usage: talkcer-cli serve [--bind 0.0.0.0:54000] [--database talkcer.sqlite] [--web apps/web/dist]"),
    }
}

async fn serve(args: &[String]) -> anyhow::Result<()> {
    let mut bind = talkcer_server::DEFAULT_BIND.to_owned();
    let mut database = PathBuf::from("talkcer.sqlite");
    let mut web = default_web_dir();
    let mut index = 0;
    while index < args.len() {
        match args[index].as_str() {
            "--bind" => {
                bind = next_value(args, &mut index, "--bind")?;
            }
            "--database" => {
                database = PathBuf::from(next_value(args, &mut index, "--database")?);
            }
            "--web" => {
                web = Some(PathBuf::from(next_value(args, &mut index, "--web")?));
            }
            other => bail!("unknown argument `{other}`"),
        }
    }

    let address: SocketAddr = bind
        .parse()
        .with_context(|| format!("invalid --bind address `{bind}`"))?;
    if let Some(dir) = web.as_ref().filter(|path| path.is_dir()) {
        tracing::info!(web = %dir.display(), "serving web bundle");
    }
    talkcer_server::serve(database, address, web).await
}

/// Default web bundle location relative to the workspace root.
///
/// The CLI runs from the repo root in development, so `apps/web/dist` is the
/// Vite build output. A missing directory is fine: the API still serves.
fn default_web_dir() -> Option<PathBuf> {
    let candidate = PathBuf::from("apps/web/dist");
    if candidate.is_dir() {
        Some(candidate)
    } else {
        None
    }
}

fn next_value(args: &[String], index: &mut usize, flag: &str) -> anyhow::Result<String> {
    *index += 1;
    let value = args
        .get(*index)
        .cloned()
        .with_context(|| format!("{flag} requires a value"))?;
    *index += 1;
    Ok(value)
}
