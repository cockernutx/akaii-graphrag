use std::sync::Arc;

use axum::extract::FromRef;

use futures::{StreamExt, TryStreamExt};
use ollama_oxide::models::{CreateResponse, PullResponse};
use ollama_oxide::{models::CreateModelRequest, OllamaClient};
use surrealdb::engine::remote::ws::{Client, Ws};
use surrealdb::opt::auth::Root;
use surrealdb::Surreal;
use thiserror::Error;
use tracing::error;

pub type Pool = Surreal<Client>;
pub type Ollama = Arc<ollama_oxide::OllamaClient>;

#[derive(Clone)]
pub struct AppState {
    pub surreal: Surreal<Client>,
    pub ollama: Ollama,
}

impl FromRef<AppState> for Pool {
    fn from_ref(input: &AppState) -> Self {
        input.surreal.clone()
    }
}
impl FromRef<AppState> for Ollama {
    fn from_ref(input: &AppState) -> Self {
        input.ollama.clone()
    }
}

const SYSTEM: &str = include_str!("triplett_system.txt");



#[derive(Debug, Error)]
pub enum Error {
    #[error("Database connection error: {0}")]
    DatabaseError(#[from] surrealdb::Error),
    #[error("Ollama connection error: {0}")]
    OllamaError(#[from] ollama_oxide::error::OllamaError),
}

impl AppState {
    pub async fn new() -> Result<Self, Error> {
        let surreal = Surreal::new::<Ws>("surrealdb:8000").await.unwrap();
        surreal
            .signin(Root {
                username: "root",
                password: "root",
            })
            .await?;
        surreal
            .use_ns("akaii-graphrag")
            .use_db("akaii-graphragdb")
            .await?;

        let ollama = OllamaClient::new("http://ollama:11434");
        let ollama = Arc::new(ollama);

        tracing::info!("downloading llm model");
    
        ollama.create_model(CreateModelRequest {
            model: "triplett".to_string(),
            from: Some("deepseek-r1:8b".to_string()),
            system: Some(SYSTEM.to_string()),
            ..Default::default()
        })
            .await?.try_collect::<Vec<CreateResponse>>().await?;

       

        tracing::info!("downloading embeddings model");
        ollama
            .pull_model("mxbai-embed-large:latest")
            .await?.try_collect::<Vec<PullResponse>>().await?;
    

        Ok(Self { surreal, ollama })
    }
}
