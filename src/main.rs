use std::sync::Arc;

use axum::routing::get;
use axum::{Json, Router};
use axum_macros::debug_handler;
use ollama_rs::{models::create::CreateModelRequest, Ollama};
use surrealdb::engine::remote::ws::Ws;
use surrealdb::opt::auth::Root;
use surrealdb::Surreal;

use std::fs::read_dir;
use std::path::{Path, PathBuf};
use tonic::transport::Server;
use tower_http::cors::CorsLayer;
use tower_http::services::ServeDir;

mod feeder_service;
mod searcher_service;

pub mod google {

    pub mod protobuf {
        use tonic::include_proto;


        include_proto!("google.protobuf");
    }
}

pub mod proto {
    use tonic::include_proto;

    include_proto!("feeder");
    include_proto!("searcher");

    pub(crate) const FILE_DESCRIPTOR_SET: &[u8] = tonic::include_file_descriptor_set!("descriptor");
}

fn recurse_protos(path: impl AsRef<Path>) -> Vec<PathBuf> {
    let Ok(entries) = read_dir(path) else {
        return vec![];
    };
    entries
        .flatten()
        .flat_map(|entry| {
            let Ok(meta) = entry.metadata() else {
                return vec![];
            };
            if meta.is_dir() {
                return recurse_protos(entry.path());
            }
            if meta.is_file() {
                return vec![entry.path()];
            }
            vec![]
        })
        .collect()
}

#[debug_handler]
async fn available_protos() -> Json<Vec<String>> {
    let protos = recurse_protos("./protos");
    let protos = protos.into_iter().map(|f| {
        let mut s = f.to_str().unwrap().to_string();
        s.remove(0);
        s
    });

    Json(protos.collect())
}

const LLM_MODELFILE: &str = r#"
    FROM gemma2:9b-instruct-q4_0
"#;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let surreal = Surreal::new::<Ws>("surrealdb:8000").await.unwrap();
    let surreal = Arc::new(surreal);
    let ollama = Ollama::new("http://ollama", 11434);
    let ollama = Arc::new(ollama);

    surreal
        .signin(Root {
            username: "root",
            password: "root",
        })
        .await
        .unwrap();
    surreal
        .use_ns("akaii-graphrag")
        .use_db("akaii-graphragdb")
        .await
        .unwrap();

    println!("downloading llm model");
    ollama
        .create_model(CreateModelRequest::modelfile(
            "llm".to_string(),
            LLM_MODELFILE.to_string(),
        ))
        .await
        .unwrap();
    println!("downloading embeddings model");
    ollama
        .pull_model("mxbai-embed-large:latest".to_string(), false)
        .await
        .unwrap();

    let service = tonic_reflection::server::Builder::configure()
        .register_encoded_file_descriptor_set(proto::FILE_DESCRIPTOR_SET)
        .build_v1alpha()
        .unwrap();

    let feeder = tonic_web::enable(proto::feeder_server::FeederServer::new(
        feeder_service::FeederService::new(surreal.clone(), ollama.clone()),
    ));
    let searcher = tonic_web::enable(proto::searcher_server::SearcherServer::new(
        searcher_service::SearcherService::new(surreal.clone(), ollama.clone()),
    ));

    let service = Server::builder()
        .accept_http1(true)
        .add_service(service)
        .add_service(feeder)
        .add_service(searcher)
        .into_service()
        .into_axum_router();

    let app = Router::new()
        .layer(CorsLayer::very_permissive())
        .nest("/grpc", service)
        .nest_service("/protos", ServeDir::new("protos"))
        .route("/available_protos", get(available_protos));

    println!("serving on 0.0.0.0:5000");
    let listener = tokio::net::TcpListener::bind("0.0.0.0:5000").await.unwrap();
    axum::serve(listener, app).await.unwrap();
    Ok(())
}
