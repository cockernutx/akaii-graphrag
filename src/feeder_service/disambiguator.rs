use std::str::FromStr;

use ollama_rs::{
    generation::embeddings::request::{EmbeddingsInput, GenerateEmbeddingsRequest},
    Ollama,
};
use serde::{Deserialize, Serialize};
use serde_json::json;
use surrealdb::{engine::remote::ws::Client, RecordId, Surreal};

use super::{Record, SimilaritySearchResult};

#[derive(Debug, Serialize, Deserialize)]
pub enum DisambiguateTableError {
    OllamaError(String),
    SurrealError(String)
}

impl From<ollama_rs::error::OllamaError> for DisambiguateTableError {
    fn from(value: ollama_rs::error::OllamaError) -> Self {
        Self::OllamaError(value.to_string())
    }
}

impl From<surrealdb::Error> for DisambiguateTableError {
    fn from(value: surrealdb::Error) -> Self {
        Self::SurrealError(value.to_string())
    }
}

#[derive(Debug, Serialize, Deserialize)]
struct TableDisambiguations {
    pub embeddings: Vec<f32>
}

pub async fn save_table_disambiguation(
    surreal: &Surreal<Client>,
    embeddings: Vec<f32>,
    record: &RecordId,
) -> Result<(), surrealdb::Error> {
    let _: Option<Record> = surreal
        .create(("_table_disambiguations_", record.to_string()))
        .content(TableDisambiguations {
            embeddings
        })
        .await?;
    Ok(())
}

pub async fn disambiguate_table(
    record: &RecordId,
    surreal: &Surreal<Client>,
    ollama: &Ollama,
) -> Result<RecordId, DisambiguateTableError> {
    let mut ret_record = record.clone();
    let embeddings = ollama
        .generate_embeddings(GenerateEmbeddingsRequest::new(
            "emb".to_string(),
            EmbeddingsInput::Single(record.clone().to_string()),
        ))
        .await?;
    let embeddings = embeddings
        .embeddings
        .into_iter()
        .flatten()
        .collect::<Vec<f32>>();

    let mut resp = surreal.query(r#"
            SELECT *, vector::similarity::cosine(embeddings, $input_vector) AS similarity OMIT embeddings FROM _table_disambiguations_ WHERE embeddings <|1|> $input_vector ORDER BY similarity DESC LIMIT 1
            "#)
            .bind(("input_vector", embeddings.clone()))
            .await?;

    let resp: Option<SimilaritySearchResult> = resp.take(0)?;

    match resp {
        Some(resp) => {
            if resp.similarity >= 0.9 {
                ret_record = RecordId::from_str(&resp.id.id.to_raw())?;
            } else {
                save_table_disambiguation(surreal, embeddings, &ret_record).await?;
            }
        }
        None => {
            save_table_disambiguation(surreal, embeddings, &ret_record).await?;
        }
    }

    Ok(ret_record)
}
