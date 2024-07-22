use aide::axum::ApiRouter;

mod docs;
mod sites;
mod system;

pub fn handler() -> ApiRouter {
	ApiRouter::new()
		.merge(docs::handler())
		.merge(sites::handler())
		.merge(system::handler())
}
