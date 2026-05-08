//! `stacks` — minimal CLI for sanity-checking a running Stacks API.
//!
//! This is intentionally thin: it covers the operations that are useful
//! during manual QA (health probe, listing watch status, listing user
//! lists) and a print-paste login flow that hands off to the browser-based
//! OIDC dance. The token is read from `STACKS_TOKEN` so users can paste it
//! after completing the web login.

use clap::{Parser, Subcommand};
use color_eyre::eyre::{Result, WrapErr, eyre};
use reqwest::Client;
use serde::Deserialize;

#[derive(Parser, Debug)]
#[command(name = "stacks", version, about = "Minimal CLI for the Stacks API")]
struct Cli {
    /// Stacks API base URL.
    #[arg(long, env = "STACKS_API", default_value = "http://localhost:8095")]
    api: String,

    /// Bearer token (JWT or API token). Required for authenticated commands.
    #[arg(long, env = "STACKS_TOKEN")]
    token: Option<String>,

    #[command(subcommand)]
    command: Command,
}

#[derive(Subcommand, Debug)]
enum Command {
    /// Probe the API's `/health` endpoint.
    Health,
    /// Print the OIDC login URL — open it in a browser, paste the resulting
    /// token into `STACKS_TOKEN` for subsequent calls.
    Login,
    /// Get the current user's profile.
    Whoami,
    /// Show watch status for a single cameo media id (e.g. `tmdb:550`).
    Status {
        #[arg(long)]
        cameo_id: String,
    },
    /// List the authenticated user's lists.
    Lists,
}

#[tokio::main]
async fn main() -> Result<()> {
    dotenvy::dotenv().ok();
    color_eyre::install()?;
    tracing_subscriber::fmt()
        .with_env_filter(tracing_subscriber::EnvFilter::from_default_env())
        .init();

    let cli = Cli::parse();
    let client = Client::new();

    match cli.command {
        Command::Health => {
            let res = client
                .get(format!("{}/health", cli.api))
                .send()
                .await?
                .text()
                .await?;
            println!("{res}");
        }
        Command::Login => {
            println!("Open: {}/api/v1/auth/login", cli.api);
            println!(
                "After authenticating, copy the access_token from the redirect \
                 and run with STACKS_TOKEN=<token>"
            );
        }
        Command::Whoami => {
            let token = cli.token.ok_or_else(|| eyre!("STACKS_TOKEN is required"))?;
            let body: serde_json::Value = client
                .get(format!("{}/api/v1/user", cli.api))
                .bearer_auth(&token)
                .send()
                .await?
                .error_for_status()
                .wrap_err("user request failed")?
                .json()
                .await?;
            println!("{}", serde_json::to_string_pretty(&body)?);
        }
        Command::Status { cameo_id } => {
            let token = cli.token.ok_or_else(|| eyre!("STACKS_TOKEN is required"))?;
            let res = client
                .get(format!("{}/api/v1/media/{}/status", cli.api, cameo_id))
                .bearer_auth(&token)
                .send()
                .await?;
            if res.status() == reqwest::StatusCode::NOT_FOUND {
                println!("(no status)");
            } else {
                let body: serde_json::Value = res
                    .error_for_status()?
                    .json()
                    .await?;
                println!("{}", serde_json::to_string_pretty(&body)?);
            }
        }
        Command::Lists => {
            let token = cli.token.ok_or_else(|| eyre!("STACKS_TOKEN is required"))?;
            #[derive(Deserialize)]
            struct ListSummary {
                id: String,
                name: String,
                item_count: u32,
            }
            let lists: Vec<ListSummary> = client
                .get(format!("{}/api/v1/users/me/lists", cli.api))
                .bearer_auth(&token)
                .send()
                .await?
                .error_for_status()?
                .json()
                .await?;
            for l in lists {
                println!("{}\t{}\t{} items", l.id, l.name, l.item_count);
            }
        }
    }

    Ok(())
}
