use crate::build;
use crate::types::{
    CreateItem, CreateItemResponse, Item, ItemListResponse, ItemQuery, ItemResponse, MessageResponse, ServerError,
    SharedState, VersionInfo,
};

use axum::extract::{Query, State};
use axum::http::StatusCode;
use axum::response::IntoResponse;
use axum::Json;
use chrono::Utc;

// Debug handler macro generates better error messages during compile
// https://docs.rs/axum-macros/latest/axum_macros/attr.debug_handler.html

/// Root returns a simple json response with the current date and time.
#[axum::debug_handler]
#[utoipa::path(
    get,
    path = "/root",
    responses(
        (status = 200, body = [MessageResponse], description = "Show current date and time")
    )
)]
pub async fn root() -> (StatusCode, Json<MessageResponse>) {
    let datetime = Utc::now().to_rfc2822();
    tracing::info!("Root: {}", datetime);
    (StatusCode::OK, Json(MessageResponse { message: datetime }))
}

/// Return version information for API.
#[axum::debug_handler]
#[utoipa::path(
    get,
    path = "/version",
    responses(
        (status = 200, body = [VersionInfo], description = "Version information")
    )
)]
pub async fn version() -> (StatusCode, Json<VersionInfo>) {
    tracing::info!("Version: {}", build::PKG_VERSION);
    (StatusCode::OK, Json(VersionInfo::from_build_info()))
}

/// Get item info.
/// Example for using query parameters.
#[axum::debug_handler]
#[utoipa::path(
    get,
    path = "/item",
    params(ItemQuery),
    responses(
        (status = 200, description = "Found existing item", body = [Item]),
        (status = 400, description = "Item does not exist", body = [MessageResponse])
    )
)]
pub async fn query_item(Query(item): Query<ItemQuery>, State(state): State<SharedState>) -> impl IntoResponse {
    tracing::info!("Query item: {}", item.name);
    let state = state.read().await;
    match state.db.get(&item.name) {
        Some(existing_item) => {
            tracing::info!("{:?}", existing_item);
            ItemResponse::Found(existing_item.clone())
        }
        None => {
            tracing::error!("Item not found: {}", item.name);
            ItemResponse::Error(MessageResponse {
                message: format!("Item does not exist: {}", item.name),
            })
        }
    }
}

/// Create new item.
/// Example for doing post with data.
#[axum::debug_handler]
#[utoipa::path(
    post,
    path = "/items",
    request_body = CreateItem,
    responses(
        (status = CREATED, body = [Item], description = "New item created"),
        (status = CONFLICT, body = [MessageResponse], description = "Item already exists")
    )
)]
pub async fn create_item(
    State(state): State<SharedState>,
    Json(payload): Json<CreateItem>,
) -> Result<CreateItemResponse, ServerError> {
    let mut state = state.write().await;
    if state.db.get(&payload.name).is_some() {
        tracing::error!("Item already exists: {}", payload.name);
        return Ok(CreateItemResponse::Error(MessageResponse::new(format!(
            "Item already exists: {}",
            payload.name
        ))));
    }
    // Check if id was provided by client
    let item = match payload.id {
        // Creating item with client provided id can fail if id is not valid,
        // which will cause this method to exit with `ServerError` due to the `?` operator.
        Some(id) => Item::new(payload.name, id)?,
        _ => Item::new_with_random_id(payload.name),
    };
    // TODO: should probably ensure ids are unique too
    state.db.insert(item.name.clone(), item.clone());
    tracing::info!("Create item: {}", item.name);
    Ok(CreateItemResponse::Created(item))
}

/// List all items
#[axum::debug_handler]
#[utoipa::path(
    get,
    path = "/list_items",
    responses(
        (status = 200, body = [ItemListResponse])
    )
)]
pub async fn list_items(State(state): State<SharedState>) -> (StatusCode, Json<ItemListResponse>) {
    tracing::debug!("List items");
    let state = state.read().await;
    let names = state.db.keys().map(|key| key.to_string()).collect::<Vec<String>>();
    let num_items = names.len();
    let response = ItemListResponse { num_items, names };
    tracing::debug!("List items: found {num_items} items");
    (StatusCode::OK, Json(response))
}
