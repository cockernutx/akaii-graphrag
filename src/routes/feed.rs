use std::{collections::HashSet, future};

use aide::axum::{
    routing::{post, post_with},
    ApiRouter,
};
use axum::{extract::State, Json};
use axum_macros::debug_handler;

use futures::{StreamExt, TryStreamExt};
use ollama_oxide::models::{EmbedInput, EmbedRequest, GenerateRequest};
use regex::Regex;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use surrealdb::{engine::remote::ws::Client, method::Query, RecordId, Surreal};
use tracing::debug;

use crate::{
    app_state::{AppState, Pool},
    shared_types::CommonError,
};

pub fn routes() -> ApiRouter<AppState> {
    ApiRouter::new().api_route(
        "/",
        post_with(feed, |t| t.response_with::<201, String, _>(|t| t)),
    )
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
struct FeedData {
    pub text: String,
    pub weight: f32,
    pub metadata: Value,
}

#[derive(Debug, Deserialize, Serialize)]
struct Entity {
    pub title: String,
    pub data: Value,
    pub title_embs: Option<Vec<f32>>,
}
#[derive(Debug, Deserialize, Serialize)]
struct Relation {
    pub from: String,
    pub to: String,
    pub relation: String,
}
#[derive(Debug, Deserialize)]
struct TextGraph {
    pub entities: Vec<Entity>,
    pub relations: Vec<Relation>,
}

fn similarity_query(surreal: &Surreal<Client>, embs: Vec<f32>) -> Query<'_, Client> {
    let query = surreal.query(r#"SELECT id.id() AS title, data, vector::similarity::cosine(title_embeddings, $embeddings) AS similarity  OMIT similarity FROM entity WHERE title_embeddings <|1,1|> $embeddings AND similarity > 0.9 LIMIT 1;"#).bind(("embeddings", embs));

    return query;
}

#[debug_handler]
async fn feed(
    State(AppState { surreal, ollama }): State<AppState>,
    Json(feed_data): Json<FeedData>,
) -> Result<String, CommonError> {
    let res = ollama
        .generate(GenerateRequest {
            model: "triplett".to_string(),
            prompt: feed_data.text.clone(),
            ..Default::default()
        })
        .await?
        .filter(|f| future::ready(f.is_ok()))
        .map_ok(|r| r.response)
        .try_collect::<Vec<String>>()
        .await?
        .join("");
    let res_text = res;

    let regex = Regex::new(r"(?s)<think>.*?</think>")?;
    let res_text = regex.replace_all(&res_text, "").to_string();
    let mut res_text = res_text.trim().trim_matches('`').to_string();
    if res_text.starts_with("json") {
        res_text = res_text.replacen("json", "", 1).trim_start().to_string();
    }
    debug!(llm_res = res_text);
    let mut text_graph: TextGraph = serde_json::from_str(&res_text)?;

    let paragraphs: Vec<String> = feed_data
        .text
        .split("\n\n") // Split by double newlines
        .map(|s| s.trim().to_string()) // Trim whitespace and convert to String
        .collect();

    let embeddings = ollama
        .generate_embeddings(EmbedRequest {
            model: "mxbai-embed-large".to_string(),
            input: EmbedInput::Multiple(paragraphs.clone()),
            ..Default::default()
        })
        .await?;
    let embeddings = embeddings.embeddings;
    let chunks: Vec<Value> = paragraphs
        .into_iter()
        .enumerate()
        .map(|(i, x)| {
            let embs = embeddings[i].clone();
            json!({
                "text": x,
                "embs": embs
            })
        })
        .collect();

    let mut query = surreal.query("BEGIN TRANSACTION");
    query = query
        .query(r#"LET $article_first = (CREATE input_article CONTENT $input_article_content)"#)
        .bind((
            "input_article_content",
            json!({
                "text": feed_data.text,
                "metadata": feed_data.metadata,
            }),
        ));
    query = query.query(
        r#"
        LET $article = $article_first[0].id;
        
        IF $article == NONE{
            THROW "could not create article";
        };
    "#,
    );

    query = query.query(r#"
        FOR $chunk IN $chunks {
            CREATE input_article_chunk SET chunk_text = $chunk.text, embeddings = $chunk.embs, article = $article;
        }
    "#).bind(("chunks", chunks));

    for entity in &mut text_graph.entities {
        let title_embs = ollama
            .generate_embeddings(EmbedRequest {
                model: "mxbai-embed-large".to_string(),
                input: EmbedInput::Single(entity.title.clone()),
                ..Default::default()
            })
            .await?;

        let mut q = similarity_query(&surreal, title_embs.embeddings[0].clone()).await?;
        let resp: Option<Entity> = q.take(0)?;
        entity.title = entity.title.replace(" ", "_").replace("'", "");
        if let Some(resp) = resp {
            let id = resp.title.clone().replace(" ", "_").replace("'", "");
            text_graph.relations.iter_mut().for_each(|x| {
                if x.to == entity.title {
                    x.to = id.clone();
                }
                if x.from == entity.title {
                    x.from = id.clone();
                }
            });
            entity.title = resp.title;
        } else {
            entity.title_embs = Some(title_embs.embeddings[0].clone())
        }
    }
    query = query
        .query(
            r#"
    FOR $entity IN $entities {
        UPSERT type::thing("entity", $entity.title) MERGE {
            data: $entity.data,
            title_embeddings: $entity.title_embs
        };
        UPDATE $article SET mentions_entity += type::thing("entity", $entity.title);
    }
    "#,
        )
        .bind(("entities", text_graph.entities));

    let mut new_entities = HashSet::new();
    for relation in text_graph.relations.iter_mut() {
        let title_embs = ollama
            .generate_embeddings(EmbedRequest {
                model: "mxbai-embed-large".to_string(),
                input: EmbedInput::Multiple(vec![relation.from.clone(), relation.to.clone()]),
                ..Default::default()
            })
            .await?;

        let embs = title_embs.embeddings;

        relation.relation = relation
            .relation
            .replace(" ", "_")
            .replace("'", "")
            .to_lowercase();

        let mut q = similarity_query(&surreal, embs[0].clone()).await?;
        let resp: Option<Entity> = q.take(0)?;

        if let Some(s) = resp {
            relation.from = s.title;
        } else {
            relation.from = relation.from.replace(" ", "_").replace("'", "");
            new_entities.insert(json!({
                "title": relation.from.clone(),
                "title_embs": embs[0].clone()
            }));
        }

        let mut q = similarity_query(&surreal, embs[1].clone()).await?;
        let resp: Option<Entity> = q.take(0)?;

        if let Some(s) = resp {
            relation.to = s.title;
        } else {
            relation.to = relation.to.replace(" ", "_").replace("'", "");
            new_entities.insert(json!({
                "title": relation.to.clone(),
                "title_embs": embs[1].clone()
            }));
        }
    }

    query = query
        .query(
            r#"
        FOR $entity IN $new_entities {
            UPSERT type::thing("entity", $entity.title) MERGE {
                title_embeddings: $entity.title_embs
            };
            UPDATE $article SET mentions_entity += type::thing("entity", $entity.title);
        }
    "#,
        )
        .bind(("new_entities", new_entities));

    query = query
        .query(
            r#"
        FOR $relation IN $relations {
            LET $from = type::thing("entity", $relation.from);
            LET $to = type::thing("entity", $relation.to);
            LET $rel = type::table($relation.relation);
            RELATE ($from) -> ($rel) -> ($to);
        };
        "#,
        )
        .bind(("relations", text_graph.relations));

    query = query.query("RETURN $article.id()");
    query = query.query(r#"COMMIT TRANSACTION"#);

    let res = query.await?;
    let mut res = res.check()?;
    let article: Option<String> = res.take(0)?;

    Ok(format!("/article/{}", article.unwrap_or_default()))
}
