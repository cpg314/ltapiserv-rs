use std::path::{Path, PathBuf};
use std::sync::Arc;

use axum::extract::{Extension, Form, Json};
use axum::response::IntoResponse;
use clap::Parser;
use log::*;
use tokio::sync::RwLock;

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
    #[clap(long, default_value_t = 50_000)]
    max_query_size: usize,
}

#[derive(thiserror::Error, Debug)]
pub enum Error {
    #[error("Unsupported language (supports {supports}, got {request})")]
    UnsupportedLanguage { supports: String, request: String },
    #[error("Missing text in request: {0:?}")]
    MissingAnnotations(anyhow::Error),
    #[error("Query too large ({0} > {1})")]
    QueryTooLarge(usize, usize),
}

impl IntoResponse for Error {
    fn into_response(self) -> axum::response::Response {
        error!("{}", self.to_string());
        (axum::http::StatusCode::BAD_REQUEST, self.to_string()).into_response()
    }
}

type CheckersExt = Extension<Arc<RwLock<Checkers>>>;

/// Main endpoint.
async fn check(
    Extension(checkers): CheckersExt,
    Extension(args): Extension<Arc<Flags>>,
    Form(request): Form<api::Request>,
) -> Result<Json<api::Response>, Error> {
    let start = std::time::Instant::now();
    info!("Received query");
    debug!("Query {:#?}", request);
    let checkers = checkers.read_owned().await;
    if request.language() != checkers.language {
        return Err(Error::UnsupportedLanguage {
            request: request.language().to_string(),
            supports: checkers.language.to_string(),
        });
    }
    let annotations = request.annotations().map_err(Error::MissingAnnotations)?;
    let text_length = annotations.text_len();
    if text_length > args.max_query_size {
        return Err(Error::QueryTooLarge(text_length, args.max_query_size));
    }

    // Process in a task
    let resp: api::Response = tokio::task::spawn_blocking(move || api::Response {
        matches: checkers.suggest(&annotations),
        language: checkers.language.clone().into(),
    })
    .await
    .unwrap();

    let elapsed_ms = start.elapsed().as_millis();
    info!(
        "Served query with {} chars in {} ms ({:.1} chars/s) with {} suggestions",
        text_length,
        elapsed_ms,
        text_length as f32 / (elapsed_ms as f32 / 1000.0),
        resp.matches.len()
    );
    Ok(resp.into())
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

    env_logger::Builder::from_env(env_logger::Env::default().default_filter_or(if args.debug {
        "debug"
    } else {
        "info"
    }))
    .filter_module("nlprule", LevelFilter::Error)
    .init();

    // Setup checkers
    let start = std::time::Instant::now();
    info!("Initializing, version {}...", env!("CARGO_PKG_VERSION"));
    let mut checkers = if let Some(archive) = &args.archive {
        Checkers::from_archive(archive)?
    } else {
        Checkers::from_archive_bytes(include_bytes!("../en_US.tar.gz"))?
    };

    // Add dictionary
    checkers.add_dictionary(Path::new(&args.dictionary))?;

    info!(
        "Done initializing {} checkers in {:?}",
        checkers.language,
        start.elapsed()
    );
    let checkers = Arc::new(RwLock::new(checkers));

    // Dictionary reloading task
    let checkers2 = checkers.clone();
    let dictionary = PathBuf::from(&args.dictionary);
    let (tx, rx) = std::sync::mpsc::channel();
    let mut debouncer =
        notify_debouncer_mini::new_debouncer(std::time::Duration::from_millis(500), tx)?;
    debouncer.watcher().watch(
        &dictionary,
        notify_debouncer_mini::notify::RecursiveMode::NonRecursive,
    )?;
    tokio::task::spawn(async move {
        loop {
            rx.recv().unwrap().unwrap();
            info!("Reloading dictionary (file changed on disk)");
            let mut checkers = checkers2.write().await;
            checkers.clear_dictionary();
            if let Err(e) = checkers.add_dictionary(&dictionary) {
                error!("Failed reloading dictionary: {}", e);
            }
        }
    });

    // Setup Axum
    let addr = std::net::SocketAddr::new(
        std::net::IpAddr::V4(std::net::Ipv4Addr::new(0, 0, 0, 0)),
        args.port,
    );
    let app = axum::Router::new()
        .route("/check", axum::routing::post(check))
        .route("/v2/check", axum::routing::post(check))
        .layer(tower_http::cors::CorsLayer::new().allow_origin(tower_http::cors::Any))
        .layer(axum::extract::Extension(checkers))
        .layer(axum::extract::Extension(Arc::new(args)));
    info!("Serving on http://{}", addr);
    let listener = tokio::net::TcpListener::bind(addr).await?;
    axum::serve(listener, app.into_make_service()).await?;

    Ok(())
}
