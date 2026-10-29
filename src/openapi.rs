//! `OpenAPI` documentation assembly.
//!
//! Keeps the documentation derive and security-scheme wiring separate from router construction,
//! so route mounting can stay focused on runtime behavior.

use utoipa::openapi::security::{ApiKey, ApiKeyValue, SecurityScheme};
use utoipa::{Modify, OpenApi};

use crate::router;
use crate::routing::{admin, routes};
use crate::types::API_KEY_HEADER;

/// `OpenAPI` documentation for the example API.
#[derive(OpenApi)]
#[openapi(
    modifiers(&SecurityAddon),
    paths(
        routes::root,
        routes::health,
        routes::status,
        routes::metrics,
        routes::version,
        routes::query_item,
        routes::list_items,
        routes::get_item,
        routes::create_item,
        routes::update_item,
        admin::delete_all_items,
        admin::remove_item,
        router::not_found,
    ),
    tags(
        (name = "service", description = "Service health, status, version, and metrics"),
        (name = "items", description = "Item CRUD examples backed by an in-memory store"),
        (name = "admin", description = "Admin routes that require the API key"),
    ),
)]
pub struct ApiDoc;

/// Document the API key in the `OpenAPI` spec.
struct SecurityAddon;

impl Modify for SecurityAddon {
    fn modify(&self, openapi: &mut utoipa::openapi::OpenApi) {
        if let Some(components) = openapi.components.as_mut() {
            components.add_security_scheme(
                "api_key",
                SecurityScheme::ApiKey(ApiKey::Header(ApiKeyValue::new(API_KEY_HEADER))),
            );
        }
    }
}
