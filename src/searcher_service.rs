use std::{collections::HashSet, sync::Arc};

use axum::http::request;
use ollama_rs::{
    generation::{
        completion::request::GenerationRequest,
        embeddings::request::{EmbeddingsInput, GenerateEmbeddingsRequest},
    },
    Ollama,
};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use surrealdb::{engine::remote::ws::Client, sql::Thing, RecordId, Surreal};
use tonic::{Request, Response, Status};
use tonic_types::ErrorDetails;

use crate::proto::{
    searcher_server::Searcher, GetNodeMessage, GetNodeResponse, SimilarText, SimilaritySearchMessage, SimilaritySearchResponse
};

pub struct SearcherService {
    surreal: Arc<Surreal<Client>>,
    ollama: Arc<Ollama>,
}

impl SearcherService {
    pub fn new(surreal: Arc<Surreal<Client>>, ollama: Arc<Ollama>) -> Self {
        Self { surreal, ollama }
    }
}

#[derive(Serialize, Deserialize)]
struct DatabaseSimilarTexts {
    id: Thing,
    text: String,
    similarity: f32,
    mentions: Vec<Thing>
}

impl Into<SimilarText> for DatabaseSimilarTexts {
    fn into(self) -> SimilarText {
        SimilarText {
            id: self.id.to_string(),
            text: self.text,
            similarity: self.similarity,
            mentions: self.mentions.into_iter().map(|f| f.to_string()).collect()
        }
    }
}

impl FromIterator<DatabaseSimilarTexts> for Vec<SimilarText> {
    fn from_iter<T: IntoIterator<Item = DatabaseSimilarTexts>>(iter: T) -> Self {
        iter.into_iter().map(|f| std::convert::Into::<SimilarText>::into(f)).collect()
    }
}

#[tonic::async_trait]
impl Searcher for SearcherService {
    async fn similarity_search(
        &self,
        request: Request<SimilaritySearchMessage>,
    ) -> Result<Response<SimilaritySearchResponse>, Status> {
        let request = request.into_inner();

        if request.minimun_similarity < 0.0 || request.minimun_similarity > 1.0 {
            return Err(Status::invalid_argument(
                "Minimun similarity has to be bigger than zero and smaller or equal than one.",
            ));
        }

        let embeddings = self
            .ollama
            .generate_embeddings(GenerateEmbeddingsRequest::new(
                "emb".to_string(),
                EmbeddingsInput::Single(request.text),
            ))
            .await
            .unwrap();
        let embeddings = embeddings.embeddings.into_iter().flatten().collect::<Vec<f32>>();
        
        let mut resp = self.surreal.query(r#"
                SELECT *, (SELECT VALUE out FROM <->?) AS mentions, vector::similarity::cosine(embeddings, $input_vector) AS similarity OMIT embeddings FROM _input_articles_ WHERE embeddings <|1|> $input_vector ORDER BY similarity DESC LIMIT 300
                "#)
                .bind(("input_vector", embeddings))
                .await.unwrap();

        let similar_texts: Vec<DatabaseSimilarTexts> = resp.take(0).unwrap();
        let similar_texts = similar_texts
            .into_iter()
            .filter(|v| v.similarity >= request.minimun_similarity)
            .collect();

        Ok(Response::new(SimilaritySearchResponse { similar_texts }))
    }

    async fn get_node(&self, request: Request<GetNodeMessage>) -> Result<Response<GetNodeResponse>, Status> {
        let request = request.into_inner();

        let record = Thing::from((request.tb, request.id));

        let mut resp = self.surreal.query(r#"
        SELECT $this.* AS content.fields, (SELECT record::tb(out) AS references.tb, record::id(out) AS references.id, weight FROM <->? ORDER BY weight) AS relations FROM $record
        "#).bind(("record", record)).await.unwrap();

        let resp: Option<GetNodeResponse> = resp.take(0).unwrap();
        match resp {
            Some(r) => Ok(Response::new(r)),
            None => Err(Status::not_found("node not found")),
        }
    }
}
