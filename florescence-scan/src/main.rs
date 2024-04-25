#![forbid(unsafe_code)]
#![doc=include_str!("../../README.md")]
#![warn(missing_docs)]
#![warn(clippy::missing_docs_in_private_items)]

/// Metadata about the crate, courtesy of [`built`]
mod built_info;
/// GraphQL resolvers
mod graphql;
/// An [`axum::handler::Handler`] for GraphQL
mod route_handlers;

use async_graphql::{http::GraphiQLSource, SDLExportOptions};
use aws_credential_types::{provider::SharedCredentialsProvider, Credentials};
use aws_sdk_s3::{config::Region, Client};
use axum::{response::Html, routing::get, Router};
use axum_tracing_opentelemetry::middleware::{OtelAxumLayer, OtelInResponseLayer};
use clap::{ArgAction::SetTrue, Parser};
use derive_more::{Deref, FromStr, Into};
use graphql::{root_schema_builder, RootSchema};
use opentelemetry_otlp::WithExportConfig;
use sea_orm::{ConnectOptions, Database, DatabaseConnection, DbErr, TransactionError};
use std::{
    fs::File,
    io::Write,
    net::{Ipv4Addr, SocketAddr, SocketAddrV4},
    path::PathBuf,
    time::Duration,
};
use tokio::net::TcpListener;
use tracing::{info, instrument};
use tracing_subscriber::{filter::FilterFn, layer::SubscriberExt, util::SubscriberInitExt};
use url::Url;

use crate::route_handlers::GraphQLHandler;

/// A service providing Beamline ISPyB data collected during sessions
#[derive(Debug, Parser)]
#[command(author, version, about, long_about=None)]
#[allow(clippy::large_enum_variant)]
enum Cli {
    /// Starts a webserver serving the GraphQL API
    Serve(ServeArgs),
    /// Produces the GraphQL schema
    Schema(SchemaArgs),
}

/// Arguments for serving the GraphQL API
#[derive(Debug, Parser)]
struct ServeArgs {
    /// The port to which this application should bind
    #[arg(short, long, env = "PORT", default_value_t = 80)]
    port: u16,
    /// The URL of the ISPyB instance which should be connected to
    #[arg(long, env = "DATABASE_URL")]
    database_url: Url,
    /// The S3 bucket which images are to be stored in.
    #[arg(long, env)]
    s3_bucket: S3Bucket,
    /// Configuration argument of the S3 client.
    #[command(flatten)]
    s3_client: S3ClientArgs,
    /// The [`tracing::Level`] to log at
    #[arg(long, env = "LOG_LEVEL", default_value_t = tracing::Level::INFO)]
    log_level: tracing::Level,
    /// The URL of the OpenTelemetry collector to send traces to
    #[arg(long, env = "OTEL_COLLECTOR_URL")]
    otel_collector_url: Option<Url>,
}

/// S3 bucket where the crystal snapshot data is stored
#[derive(Debug, Clone, Deref, FromStr, Into)]
pub struct S3Bucket(String);

/// Arguments for configuring the S3 Client.
#[derive(Debug, Parser)]
pub struct S3ClientArgs {
    /// The URL of the S3 endpoint to retrieve images from.
    #[arg(long, env)]
    s3_endpoint_url: Option<Url>,
    /// The ID of the access key used for S3 authorization.
    #[arg(long, env)]
    s3_access_key_id: Option<String>,
    /// The secret access key used for S3 authorization.
    #[arg(long, env)]
    s3_secret_access_key: Option<String>,
    /// Forces path style endpoint URIs for S3 queries.
    #[arg(long, env, action = SetTrue)]
    s3_force_path_style: bool,
    /// The AWS region of the S3 bucket.
    #[arg(long, env)]
    s3_region: Option<String>,
}

/// S3 client argument trait
pub trait FromS3ClientArgs {
    /// Creates a S3 [`Client`] with the supplied credentials using the supplied endpoint configuration.
    fn from_s3_client_args(args: S3ClientArgs) -> Self;
}

impl FromS3ClientArgs for Client {
    fn from_s3_client_args(args: S3ClientArgs) -> Self {
        let credentials = Credentials::new(
            args.s3_access_key_id.unwrap_or_default(),
            args.s3_secret_access_key.unwrap_or_default(),
            None,
            None,
            "Other",
        );
        let credentials_provider = SharedCredentialsProvider::new(credentials);
        let mut config_builder = aws_sdk_s3::config::Builder::new();
        config_builder.set_credentials_provider(Some(credentials_provider));
        config_builder.set_endpoint_url(args.s3_endpoint_url.map(String::from));
        config_builder.set_force_path_style(Some(args.s3_force_path_style));
        config_builder.set_region(Some(Region::new(
            args.s3_region.unwrap_or(String::from("undefined")),
        )));
        let config = config_builder.build();
        Client::from_conf(config)
    }
}

/// Arguments for produces the GraphQL schema
#[derive(Debug, Parser)]
struct SchemaArgs {
    /// The path to write the schema to, if not set the schema will be printed to stdout
    #[arg(short, long)]
    path: Option<PathBuf>,
    /// The URL of the ISPyB instance which should be connected to
    #[arg(long, env = "DATABASE_URL")]
    database_url: Url,
}

/// Creates a connection pool to access the database
#[instrument(skip(database_url))]
async fn setup_database(database_url: Url) -> Result<DatabaseConnection, TransactionError<DbErr>> {
    info!("Connecting to database at {database_url}");
    let connection_options = ConnectOptions::new(database_url.to_string())
        .sqlx_logging_level(tracing::log::LevelFilter::Debug)
        .to_owned();
    let connection = Database::connect(connection_options).await?;
    info!("Database connection established: {connection:?}");
    Ok(connection)
}

/// Creates an [`axum::Router`] serving GraphiQL, synchronous GraphQL and GraphQL subscriptions
fn setup_router(
    schema: RootSchema,
    database: DatabaseConnection,
    s3_client: Client,
    s3_bucket: S3Bucket,
) -> Router {
    #[allow(clippy::missing_docs_in_private_items)]
    const GRAPHQL_ENDPOINT: &str = "/";

    Router::new()
        .route(
            GRAPHQL_ENDPOINT,
            get(Html(
                GraphiQLSource::build().endpoint(GRAPHQL_ENDPOINT).finish(),
            ))
            .post(GraphQLHandler::new(schema, database, s3_client, s3_bucket)),
        )
        .layer(OtelInResponseLayer)
        .layer(OtelAxumLayer::default())
}

/// Serves the endpoints on the specified port forever
async fn serve(router: Router, port: u16) -> Result<(), std::io::Error> {
    let socket_addr = SocketAddr::V4(SocketAddrV4::new(Ipv4Addr::UNSPECIFIED, port));
    let listener = TcpListener::bind(socket_addr).await?;
    println!("Serving API & GraphQL UI at {}", socket_addr);
    axum::serve(listener, router.into_make_service()).await?;
    Ok(())
}

/// Sets up Logging & Tracing using opentelemetry if available
fn setup_telemetry(
    log_level: tracing::Level,
    otel_collector_url: Option<Url>,
) -> Result<(), anyhow::Error> {
    let custom_filter = FilterFn::new(|metadata| {
        !metadata.target().contains("aws_smithy_runtime")
            && !metadata.target().contains("aws_credential_types")
    });
    let level_filter = tracing_subscriber::filter::LevelFilter::from_level(log_level);
    let log_layer = tracing_subscriber::fmt::layer();
    let service_name_resource = opentelemetry_sdk::Resource::new(vec![
        opentelemetry::KeyValue::new(
            opentelemetry_semantic_conventions::resource::SERVICE_NAME,
            built_info::PKG_NAME,
        ),
        opentelemetry::KeyValue::new(
            opentelemetry_semantic_conventions::resource::SERVICE_VERSION,
            built_info::PKG_VERSION,
        ),
    ]);
    let (metrics_layer, tracing_layer) = if let Some(otel_collector_url) = otel_collector_url {
        opentelemetry::global::set_text_map_propagator(
            opentelemetry_sdk::propagation::TraceContextPropagator::default(),
        );
        (
            Some(tracing_opentelemetry::MetricsLayer::new(
                opentelemetry_otlp::new_pipeline()
                    .metrics(opentelemetry_sdk::runtime::Tokio)
                    .with_exporter(
                        opentelemetry_otlp::new_exporter()
                            .tonic()
                            .with_endpoint(otel_collector_url.clone()),
                    )
                    .with_resource(service_name_resource.clone())
                    .with_period(Duration::from_secs(10))
                    .build()?,
            )),
            Some(
                tracing_opentelemetry::layer().with_tracer(
                    opentelemetry_otlp::new_pipeline()
                        .tracing()
                        .with_exporter(
                            opentelemetry_otlp::new_exporter()
                                .tonic()
                                .with_endpoint(otel_collector_url),
                        )
                        .with_trace_config(
                            opentelemetry_sdk::trace::config().with_resource(service_name_resource),
                        )
                        .install_batch(opentelemetry_sdk::runtime::Tokio)?,
                ),
            ),
        )
    } else {
        (None, None)
    };

    tracing_subscriber::Registry::default()
        .with(custom_filter)
        .with(level_filter)
        .with(log_layer)
        .with(metrics_layer)
        .with(tracing_layer)
        .init();

    Ok(())
}

#[tokio::main]
async fn main() {
    dotenvy::dotenv().ok();
    let args = Cli::parse();

    match args {
        Cli::Serve(args) => {
            setup_telemetry(args.log_level, args.otel_collector_url).unwrap();
            let database = setup_database(args.database_url).await.unwrap();
            let s3_client = Client::from_s3_client_args(args.s3_client);
            let schema = root_schema_builder().finish();
            let router = setup_router(schema, database, s3_client, args.s3_bucket);
            serve(router, args.port).await.unwrap();
        }
        Cli::Schema(args) => {
            let schema = root_schema_builder().finish();
            let schema_string = schema.sdl_with_options(SDLExportOptions::new().federation());
            if let Some(path) = args.path {
                let mut file = File::create(path).unwrap();
                file.write_all(schema_string.as_bytes()).unwrap();
            } else {
                println!("{}", schema_string)
            }
        }
    }
}
