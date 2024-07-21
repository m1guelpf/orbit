use aide::{axum::ApiRouter, openapi::OpenApi, scalar::Scalar};
use axum::{routing::get, Extension, Json};

pub fn handler() -> ApiRouter {
	let scalar = Scalar::new("/openapi.json").with_title("Orbit Docs");

	ApiRouter::new()
		.route("/docs", scalar.axum_route())
		.route("/openapi.json", get(openapi_schema))
}

#[allow(clippy::unused_async)]
async fn openapi_schema(Extension(openapi): Extension<OpenApi>) -> Json<OpenApi> {
	Json(openapi)
}
