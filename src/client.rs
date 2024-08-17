use std::path::PathBuf;

use anyhow::Context;
use clap::Parser;
use itertools::Itertools;
use log::*;

use ltapiserv_rs::api::{Request, Response};

/// Run text through a LanguageTool server and display the results.
#[derive(Parser)]
struct Flags {
    /// Filename; if not provided, will read from stdin.
    filename: Option<PathBuf>,
    #[clap(long, short, default_value = "en-US")]
    language: String,
    /// Server base URL (e.g. http://localhost:8875)
    #[clap(long, short, env = "LTAPI_SERVER")]
    server: reqwest::Url,
    /// JSON output
    #[clap(long)]
    json: bool,
    /// Number of suggestions to display
    #[clap(long, default_value_t = 3)]
    suggestions: usize,
    /// Convert to plaintext with pandoc, removing code blocks. Line numbers are not preserved.
    #[clap(long, requires = "filename")]
    pandoc: bool,
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    if let Err(err) = main_impl().await {
        error!("{}", err);
        std::process::exit(1);
    }
    Ok(())
}
async fn main_impl() -> anyhow::Result<()> {
    let args = Flags::parse();

    env_logger::Builder::from_env(env_logger::Env::default().default_filter_or("info"))
        .filter_module("nlprule", LevelFilter::Error)
        .init();

    let text = if let Some(filename) = &args.filename {
        if args.pandoc {
            info!("Converting to plain text with pandoc");
            let filter = tempfile::NamedTempFile::new()?;
            std::fs::write(filter.path(), include_str!("filter.lua"))?;
            let out = std::process::Command::new("pandoc")
                .arg(filename)
                .args(["--to", "plain", "--lua-filter"])
                .arg(filter.path())
                .stdout(std::process::Stdio::piped())
                .output()
                .context("pandoc not found")?;
            anyhow::ensure!(out.status.success(), "pandoc did not execute successfully");
            String::from_utf8_lossy(&out.stdout).to_string()
        } else {
            std::fs::read_to_string(filename)
                .with_context(|| format!("Could not open {:?}", filename))?
        }
    } else {
        info!("Reading from stdin",);
        std::io::read_to_string(std::io::stdin())?
    };
    debug!("Text to process: {}", text);

    // Request and read results
    let endpoint = args.server.join("v2/check")?;
    info!("Sending request to {}", endpoint);
    let start = std::time::Instant::now();
    let client = reqwest::Client::new();
    let request = Request::new(text.clone(), &args.language);
    let resp: Response = client
        .post(endpoint)
        .form(&request)
        .send()
        .await?
        .error_for_status()?
        .json()
        .await?;
    info!("Received response in {:?}", start.elapsed());

    let n_errors = resp.matches.len();
    if args.json {
        println!("{}", serde_json::to_string_pretty(&resp)?);
    } else {
        // Report errors
        if resp.matches.is_empty() {
            info!("No errors found");
            return Ok(());
        }
        let text = std::sync::Arc::new(text);
        for m in resp.matches {
            // Get the byte offsets for miette
            let start = text.char_indices().nth(m.offset).unwrap().0;
            let end = text.char_indices().nth(m.offset + m.length).unwrap().0;
            let report = miette::miette!(
                severity = if m.rule.is_spelling() {
                    miette::Severity::Warning
                } else {
                    miette::Severity::Advice
                },
                labels = vec![miette::LabeledSpan::at(
                    start..end,
                    m.replacements
                        .into_iter()
                        .take(args.suggestions)
                        .map(|r| r.value)
                        .join(" / ")
                ),],
                // code = m.rule.id,
                "{}",
                m.message,
            )
            .with_source_code(text.clone());
            println!("{:?}", report);
        }
    }
    info!("Found {} potential errors", n_errors);
    if n_errors > 0 {
        std::process::exit(1);
    }

    Ok(())
}
