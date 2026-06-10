//! `OpenAPI` documentation assembly.
//!
//! Keeps the documentation derive and security-scheme wiring separate from
//! router construction, so route mounting can stay focused on runtime behavior.

use utoipa::openapi::security::{ApiKey, ApiKeyValue, SecurityScheme};
use utoipa::{Modify, OpenApi};

use crate::router;
use crate::routing::admin;
use crate::routing::routes;

/// `OpenAPI` documentation for the example API.
#[derive(OpenApi)]
#[openapi(
    modifiers(&SecurityAddon),
    paths(
        routes::root,
        routes::health,
        routes::metrics,
        routes::version,
        routes::query_item,
        routes::list_items,
        routes::create_item,
        admin::delete_all_items,
        admin::remove_item,
        router::not_found,
    ),
)]
pub struct ApiDoc;

/// Document api key in `OpenAPI` specs.
struct SecurityAddon;

impl Modify for SecurityAddon {
    fn modify(&self, openapi: &mut utoipa::openapi::OpenApi) {
        if let Some(components) = openapi.components.as_mut() {
            components.add_security_scheme(
                "api_key",
                SecurityScheme::ApiKey(ApiKey::Header(ApiKeyValue::new("api-key"))),
            );
        }
    }
}
