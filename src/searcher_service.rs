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
    searcher_server::Searcher, Node, GetNodeResponse, SimilarText, SimilaritySearchMessage, SimilaritySearchResponse
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
                SELECT *, type::string(id) AS id, (SELECT record::tb(out) AS tb, record::id(out) AS id FROM <->?) AS mentions, vector::similarity::cosine(embeddings, $input_vector) AS similarity OMIT embeddings FROM _input_articles_ WHERE embeddings <|1|> $input_vector ORDER BY similarity DESC LIMIT 300
                "#)
                .bind(("input_vector", embeddings))
                .await.unwrap();

        let similar_texts: Vec<SimilarText> = resp.take(0).unwrap();
    

        Ok(Response::new(SimilaritySearchResponse { similar_texts }))
    }

    async fn get_node(&self, request: Request<Node>) -> Result<Response<GetNodeResponse>, Status> {
        let request = request.into_inner();

        let record = Thing::from((request.tb, request.id));

        let mut resp = self.surreal.query(r#"
        SELECT (SELECT IF out == $record
            { {
                id: record::id($parent.in),
                tb: record::tb($parent.in)
            } }
        ELSE
            { {
                id: record::id($parent.out),
                tb: record::tb($parent.out)
            } }
         AS references, weight, record::tb(id) AS relation_type FROM <->? ORDER BY weight ) AS relations FROM $record;
        "#).bind(("record", record.clone())).await.unwrap();

        let resp: Option<GetNodeResponse> = resp.take(0).unwrap();
        match resp {
            Some(mut node_resp) => {
                let mut resp = self.surreal.query(r#"
                BEGIN TRANSACTION;
                LET $obj = SELECT * FROM $record;
                
                RETURN function($obj) {
                    let [obj] = arguments;
                    return JSON.stringify(obj[0])
                };
                COMMIT TRANSACTION;
                "#).bind(("record", record)).await.unwrap();
                let Ok(content) = resp.take::<Option<String>>(0) else {
                    return Err(Status::internal("error executing query on database"))
                };
                node_resp.content = content;
                Ok(Response::new(node_resp))
            },
            None => Err(Status::not_found("node not found")),
        }
    }
}
