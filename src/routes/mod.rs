use aide::axum::ApiRouter;

mod docs;
mod system;

pub fn handler() -> ApiRouter {
	ApiRouter::new()
		.merge(docs::handler())
		.merge(system::handler())
}
