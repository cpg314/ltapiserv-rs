use std::path::{Path, PathBuf};
use std::sync::Arc;

use axum::extract::{Extension, Form, Json};
use axum::response::IntoResponse;
use clap::Parser;
use log::*;

use ltapiserv_rs::api;
use ltapiserv_rs::checkers::Checkers;

fn dictionary() -> String {
    dirs::data_dir()
        .map(|d| d.join("ltapiserv-rs").join("dictionary.txt"))
        .and_then(|d| d.to_str().map(String::from))
        .unwrap_or_default()
}

/// Alternative API server for LanguageTool
#[derive(Parser)]
#[clap(version)]
struct Flags {
    /// Path to a .tar.gz data archive. If not provided, the data will be loaded from the binary.
    #[clap(long)]
    archive: Option<PathBuf>,
    /// Path to custom dictionary
    #[clap(long, default_value_t = dictionary())]
    dictionary: String,
    #[clap(long, default_value_t = 8875)]
    port: u16,
    /// Verbose logging
    #[clap(long, short)]
    debug: bool,
}

#[derive(thiserror::Error, Debug)]
pub enum Error {
    #[error("Unsupported language (supports {supports}, got {request})")]
    UnsupportedLanguage { supports: String, request: String },
    #[error("Missing text in request: {0:?}")]
    MissingAnnotations(anyhow::Error),
}

impl IntoResponse for Error {
    fn into_response(self) -> axum::response::Response {
        error!("{}", self.to_string());
        (axum::http::StatusCode::BAD_REQUEST, self.to_string()).into_response()
    }
}

/// Main endpoint.
async fn check(
    Form(request): Form<api::Request>,
    Extension(checkers): Extension<Arc<Checkers>>,
) -> Result<Json<api::Response>, Error> {
    let start = std::time::Instant::now();
    info!("Received query");
    debug!("Query {:#?}", request);
    if request.language() != checkers.language {
        return Err(Error::UnsupportedLanguage {
            request: request.language().to_string(),
            supports: checkers.language.to_string(),
        });
    }
    let annotations = request.annotations().map_err(Error::MissingAnnotations)?;
    let text_length = annotations.text_len();

    // Process in a task
    let resp: api::Response = tokio::task::spawn_blocking(move || api::Response {
        matches: checkers.suggest(&annotations),
        language: checkers.language.clone().into(),
    })
    .await
    .unwrap();

    let elapsed_ms = start.elapsed().as_millis();
    info!(
        "Served query in {} ms ({:.1} chars/s) with {} suggestions",
        elapsed_ms,
        text_length as f32 / (elapsed_ms as f32 / 1000.0),
        resp.matches.len()
    );
    Ok(resp.into())
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let args = Flags::parse();
    // Setup logging
    let colors = fern::colors::ColoredLevelConfig::new()
        .debug(fern::colors::Color::Blue)
        .info(fern::colors::Color::Green)
        .error(fern::colors::Color::Red)
        .warn(fern::colors::Color::Yellow);
    fern::Dispatch::new()
        .format(move |out, message, record| {
            out.finish(format_args!(
                "{} {} [{}] {}",
                chrono::Local::now().format("[%Y-%m-%d %H:%M:%S]"),
                colors.color(record.level()),
                record.target(),
                message,
            ))
        })
        .level(if args.debug {
            log::LevelFilter::Debug
        } else {
            log::LevelFilter::Info
        })
        .level_for("nlprule", log::LevelFilter::Error)
        .chain(std::io::stdout())
        .apply()?;

    // Setup checkers
    let start = std::time::Instant::now();
    info!("Initializing...");
    let mut checkers = if let Some(archive) = args.archive {
        Checkers::from_archive(&archive)?
    } else {
        Checkers::from_archive_bytes(include_bytes!("../en_US.tar.gz"))?
    };

    // Add dictionary
    checkers.add_dictionary(Path::new(&args.dictionary))?;

    let checkers = Arc::new(checkers);
    info!(
        "Done initializing {} checkers in {:?}",
        checkers.language,
        start.elapsed()
    );

    // Setup Axum
    let app = axum::Router::new()
        .route("/check", axum::routing::post(check))
        .route("/v2/check", axum::routing::post(check))
        .layer(tower_http::cors::CorsLayer::new().allow_origin(tower_http::cors::Any))
        .layer(axum::extract::Extension(checkers));
    let addr = std::net::SocketAddr::new(
        std::net::IpAddr::V4(std::net::Ipv4Addr::new(0, 0, 0, 0)),
        args.port,
    );
    info!("Serving on http://{}", addr);
    axum::Server::bind(&addr)
        .serve(app.into_make_service())
        .await?;

    Ok(())
}
