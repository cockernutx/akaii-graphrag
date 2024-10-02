use std::{
    collections::HashSet,
    sync::Arc,
};

use disambiguator::disambiguate_table;
use ollama_rs::{
    generation::{
        completion::request::GenerationRequest,
        embeddings::request::{EmbeddingsInput, GenerateEmbeddingsRequest},
    },
    Ollama,
};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use surrealdb::{engine::remote::ws::Client, RecordId, Surreal};
use tonic::{Request, Response, Status};
use tonic_types::ErrorDetails;

use crate::proto::{FeedMessage, FeedResponse};

use super::proto::feeder_server::Feeder;

mod disambiguator;

pub struct FeederService {
    surreal: Arc<Surreal<Client>>,
    ollama: Arc<Ollama>,
}

impl FeederService {
    pub fn new(surreal: Arc<Surreal<Client>>, ollama: Arc<Ollama>) -> Self {
        Self { surreal, ollama }
    }
}

#[derive(Serialize, Deserialize, Clone)]
struct RelationObject {
    pub table: String,
    pub id: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub content: Option<Value>,
}

#[derive(Serialize, Deserialize, Clone)]
struct Relation {
    pub r#in: RelationObject,
    pub relation: String,
    pub out: RelationObject,
}

#[derive(Serialize, Deserialize, Clone)]
struct InputArticle {
    pub text: String,
    pub embeddings: Vec<f32>,
}

#[derive(Serialize, Deserialize, Clone)]
struct SimilaritySearchResult {
    #[allow(dead_code)]
    id: surrealdb::sql::Thing,
    similarity: f32,
}

#[derive(Debug, Deserialize)]
struct Record {
    #[allow(dead_code)]
    id: surrealdb::sql::Thing,
}

const FEED_PROMPT: &str = include_str!("feeder_service/feed_prompt.txt");



#[tonic::async_trait]
impl Feeder for FeederService {
    async fn feed(&self, request: Request<FeedMessage>) -> Result<Response<FeedResponse>, Status> {
        let request = request.into_inner();
        let message = request.text;
        let weight = request.weight;
        let prompt = format!(
            r#"
        {FEED_PROMPT} 
        
        Apply the previous instructions to the following text, only the json object should be generated:
        "
        {message}
        "
        "#
        );
        let res = self
            .ollama
            .generate(GenerationRequest::new("llm".to_string(), prompt))
            .await
            .unwrap();
        let res_text = {
            let t = res.response;
            let mut t = t.trim().to_string().split_off(7);
            t.pop();
            t.pop();
            t.pop();

            t
        };
        let relations: Vec<Relation> = serde_json::from_str(&res_text).unwrap();
    
        let embeddings = self
            .ollama
            .generate_embeddings(GenerateEmbeddingsRequest::new(
                "emb".to_string(),
                EmbeddingsInput::Single(message.clone()),
            ))
            .await
            .unwrap();

        let article_record: Option<Record> = self
            .surreal
            .create("_input_articles_")
            .content(InputArticle {
                text: message,
                embeddings: embeddings.embeddings.into_iter().flatten().collect(),
            })
            .await
            .unwrap();
        let article_record = article_record.unwrap();
        let article_record = RecordId::from((article_record.id.tb, article_record.id.id.to_raw()));
        let mut input_article_mentions = HashSet::new();
        for relation in relations {
            let r#in = relation.r#in;
            let out = relation.out;
            let in_record = RecordId::from_table_key(r#in.table, r#in.id);
            let in_record = disambiguate_table(&in_record, &self.surreal, &self.ollama).await.unwrap();

            let out_record = RecordId::from_table_key(out.table, out.id);
            let out_record = disambiguate_table(&out_record, &self.surreal, &self.ollama).await.unwrap();
            
            let relation = relation.relation;
            let _: Option<Record> = self
                .surreal
                .upsert(in_record.clone())
                .content(r#in.content)
                .await
                .unwrap();
            let _: Option<Record> = self
                .surreal
                .upsert(out_record.clone())
                .content(out.content)
                .await
                .unwrap();

            self.surreal
                .query(r#"
                RELATE (type::record($in_record)) -> (type::table($relation)) -> (type::record($out_record)) SET weight = IF weight {math::clamp(0, weight, 1 * $w)} ELSE {math::clamp(0, 1, 1 * $w)} "#)
                .bind(("in_record".to_string(), in_record.clone()))
                .bind(("relation".to_string(), relation))
                .bind(("out_record".to_string(), out_record.clone()))
                .bind(("w", weight)).await.unwrap();

            input_article_mentions.insert(in_record.to_string());

            input_article_mentions.insert(out_record.to_string());
        }

        for mention in input_article_mentions {
            self.surreal.query(r#"RELATE (type::record($article)) -> _input_article_mentions_ -> (type::record($mention)) SET weight = IF weight {math::clamp(0, weight, 1 * $w)} ELSE {math::clamp(0, 1, 1 * $w)}"#)
            .bind(("article".to_string(), article_record.clone()))
            .bind(("mention".to_string(), mention))
            .bind(("w", weight)).await.unwrap();
        }

        Ok(Response::new(FeedResponse { article_id: article_record.to_string() }))
    }
}
