use aide::axum::{routing::get, ApiRouter};
use axum_jsonschema::Json;
use schemars::JsonSchema;

pub fn handler() -> ApiRouter {
	ApiRouter::new().api_route("/", get(root))
}

#[derive(Debug, serde::Serialize, JsonSchema)]
pub struct AppVersion {
	/// Current version of the application
	semver: String,
	/// Commit hash of the current build (if available)
	rev: Option<String>,
	/// The time the application was compiled at
	compile_time: String,
}

#[derive(Debug, serde::Serialize, JsonSchema)]
pub struct RootResponse {
	/// Relative URL to Swagger UI
	pub docs_url: String,
	/// Relative URL to `OpenAPI` specification
	pub openapi_url: String,
	/// Application version
	pub version: AppVersion,
}

#[allow(clippy::unused_async)]
pub async fn root() -> Json<RootResponse> {
	Json(RootResponse {
		docs_url: "/docs".to_string(),
		openapi_url: "/openapi.json".to_string(),
		version: AppVersion {
			semver: env!("CARGO_PKG_VERSION").to_string(),
			compile_time: env!("STATIC_BUILD_DATE").to_string(),
			rev: option_env!("GIT_REV").map(ToString::to_string),
		},
	})
}
