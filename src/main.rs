use aide::axum::routing::get;
use aide::openapi::{Info, OpenApi};

use aide::axum::{ApiRouter, IntoApiResponse};
use aide::scalar::Scalar;
use app_state::AppState;
use axum::response::IntoResponse;
use axum::{Extension, Json};

mod app_state;
mod routes;
mod shared_types;

async fn serve_api(Extension(api): Extension<OpenApi>) -> impl IntoApiResponse {
    Json(api).into_response()
}

#[tokio::main]
async fn main() {
    tracing_subscriber::fmt()
        .with_max_level(tracing::Level::DEBUG)
        .init();

    tracing::info!("starting application");

    let mut api = OpenApi {
        info: Info {
            description: Some("Akaii Graphrag Api v1".to_string()),
            ..Default::default()
        },
        ..Default::default()
    };

    let app = ApiRouter::new()
        .route("/scalar", Scalar::new("/api.json").axum_route())
        .route("/api.json", get(serve_api))
        .nest("/", routes::routes())
        .with_state(AppState::new().await.expect("could not start appstate"));
    let app = app.finish_api(&mut api).layer(Extension(api)).into_make_service();

    tracing::info!("serving on 0.0.0.0:5000");
    let listener = tokio::net::TcpListener::bind("0.0.0.0:5000").await.unwrap();
    axum::serve(listener, app).await.unwrap();
}
