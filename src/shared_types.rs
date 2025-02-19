use core::error;

use aide::OperationIo;
use axum_error_handler::AxumErrorResponse;
use serde::{Deserialize, Serialize};
use surrealdb::sql::Thing;
use thiserror::Error;

#[derive(Debug, Serialize, Deserialize)]
pub struct Record {
    pub id: Thing,
}

#[derive(Debug, Error, AxumErrorResponse, OperationIo)]
pub enum CommonError {
    #[error("database error: {0}")]
    #[status_code("500")]
    DatabaseError(#[from] surrealdb::Error),
    #[error("ollama error: {0}")]
    #[status_code("500")]
    OllamaError(#[from] ollama_oxide::error::OllamaError),
    #[error("regex error: {0}")]
    #[status_code("500")]
    RegexError(#[from] regex::Error),
    #[error("serde error: {0}")]
    #[status_code("500")]
    SerdeError(#[from] serde_json::Error)
}