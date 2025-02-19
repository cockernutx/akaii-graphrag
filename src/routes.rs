use aide::axum::ApiRouter;
use crate::app_state::AppState;

mod feed;

pub fn routes() -> ApiRouter<AppState> {
    ApiRouter::new().nest("/feed", feed::routes())
}