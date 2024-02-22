use crate::types::SharedState;

use axum::{
    extract::{Path, State},
    routing::delete,
    Router,
};

/// Create admin routes
pub fn admin_routes() -> Router<SharedState> {
    Router::new()
        .route("/clear_users", delete(delete_all_users))
        .route("/remove/:key", delete(remove_user))
}

async fn delete_all_users(State(state): State<SharedState>) {
    let mut state = state.write().await;
    state.db.clear();
}

async fn remove_user(Path(key): Path<String>, State(state): State<SharedState>) {
    let mut state = state.write().await;
    state.db.remove(&key);
}
