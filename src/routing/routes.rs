use axum::extract::{Query, State};
use axum::http::StatusCode;
use axum::response::IntoResponse;
use axum::Json;
use axum_extra::extract::WithRejection;
use chrono::{SecondsFormat, Utc};

use crate::build;
use crate::schemas::{
    CreateItem, CreateItemResponse, ItemListResponse, ItemQuery, ItemResponse, MessageResponse, RejectionError,
    ServerError, VersionInfo, VERSION_INFO,
};
use crate::types::{Item, SharedState};

// Debug handler macro generates better error messages during compile
// https://docs.rs/axum-macros/latest/axum_macros/attr.debug_handler.html

/// Returns a simple JSON response with API name together with the current date and time.
/// Used primarily as a health check to verify the API is up and responding.
#[axum::debug_handler]
#[utoipa::path(
    get,
    path = "/",
    responses(
        (status = OK, body = [MessageResponse], description = "Return API name with current datetime")
    )
)]
pub async fn root() -> (StatusCode, Json<MessageResponse>) {
    let datetime = Utc::now().to_rfc3339_opts(SecondsFormat::Secs, true);
    tracing::debug!("Root: {}", datetime);
    (
        StatusCode::OK,
        Json(MessageResponse::new(format!("{} {}", build::PROJECT_NAME, datetime))),
    )
}

/// Return version and build information for the API.
#[axum::debug_handler]
#[utoipa::path(
    get,
    path = "/version",
    responses(
        (status = OK, body = [VersionInfo], description = "Version information")
    )
)]
pub async fn version() -> (StatusCode, Json<&'static VersionInfo>) {
    tracing::debug!("Version: {}", build::PKG_VERSION);
    (StatusCode::OK, Json(&VERSION_INFO))
}

/// Get item info.
/// Example for using query parameters.
#[axum::debug_handler]
#[utoipa::path(
    get,
    path = "/item",
    params(ItemQuery),
    responses(
        (status = 200, body = [Item], description = "Found existing item"),
        (status = 400, body = [MessageResponse], description = "Item does not exist")
    )
)]
pub async fn query_item(Query(item): Query<ItemQuery>, State(state): State<SharedState>) -> impl IntoResponse {
    tracing::debug!("Query item: {}", item.name);
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
        (status = CONFLICT, body = [MessageResponse], description = "Item already exists"),
        (status = BAD_REQUEST, body = [RejectionErrorResponse], description = "Malformed JSON data"),
        (status = UNPROCESSABLE_ENTITY, body = [RejectionErrorResponse], description = "JSON deserialization error"),
        (status = UNSUPPORTED_MEDIA_TYPE, body = [RejectionErrorResponse], description = "Missing JSON content type header"),
        (status = PAYLOAD_TOO_LARGE, body = [RejectionErrorResponse], description = "Too many bytes"),
    )
)]
pub async fn create_item(
    State(state): State<SharedState>,
    WithRejection(Json(payload), _): WithRejection<Json<CreateItem>, RejectionError>,
) -> Result<CreateItemResponse, ServerError> {
    let mut state = state.write().await;
    if state.db.contains_key(&payload.name) {
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
    tracing::debug!("Create item: {}", item.name);
    Ok(CreateItemResponse::Created(item))
}

/// List all items.
// TODO: add optional parameters like skip and limit
#[axum::debug_handler]
#[utoipa::path(
    get,
    path = "/items",
    responses(
        (status = 200, body = [ItemListResponse])
    )
)]
pub async fn list_items(State(state): State<SharedState>) -> (StatusCode, Json<ItemListResponse>) {
    tracing::debug!("List items");
    let state = state.read().await;
    let names: Vec<String> = state.db.keys().map(|key| key.to_string()).collect();
    let num_items = names.len();
    tracing::debug!("List items: found {num_items} items");
    (StatusCode::OK, Json(ItemListResponse { num_items, names }))
}
