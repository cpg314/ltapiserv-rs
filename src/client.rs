use std::path::PathBuf;

use ariadne::{Color, Label, Report, ReportKind, Source};
use clap::Parser;
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
        std::fs::read_to_string(filename)?
    } else {
        info!("Reading from stdin",);
        std::io::read_to_string(std::io::stdin())?
    };

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

    if args.json {
        println!("{}", serde_json::to_string_pretty(&resp)?);
    } else {
        // Report errors
        let filename = args
            .filename
            .map(|f| f.to_string_lossy().into_owned())
            .unwrap_or("stdin".to_string());
        let mut report = Report::build(ReportKind::Error, &filename, 0)
            .with_config(ariadne::Config::default().with_compact(true));
        if resp.matches.is_empty() {
            info!("No errors found");
            return Ok(());
        }
        for m in &resp.matches {
            report = report.with_label(
                Label::new((&filename, m.offset..m.offset + m.length))
                    .with_message(&m.message)
                    .with_color(if m.rule.is_spelling() {
                        Color::Green
                    } else {
                        Color::Red
                    }),
            );
        }
        report.finish().print((&filename, Source::from(text)))?;
    }
    if !resp.matches.is_empty() {
        std::process::exit(1);
    }

    Ok(())
}
