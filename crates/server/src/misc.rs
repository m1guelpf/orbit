use aide::{
	openapi::{MediaType, Operation, Response, SchemaObject},
	r#gen::GenContext,
};
use async_fn_stream::try_fn_stream;
use axum::response::{
	sse::{Event, KeepAlive},
	IntoResponse,
};
use futures_util::Stream;
use indexmap::IndexMap;
use orbit_types::Log;
use schemars::JsonSchema;
use std::{
	fs,
	io::{self, Read},
	path::Path,
	process::Stdio,
	rc::Rc,
};
use tokio::{
	io::{AsyncBufReadExt, BufReader},
	process::Command,
};

#[derive(Debug)]
#[repr(transparent)]
pub struct Sse<S>(axum::response::Sse<S>);

impl<S> Sse<S> {
	/// Create a new [`Sse`] response that will respond with the given stream of
	/// [`Event`]s.
	///
	/// See the [module docs](self) for more details.
	pub fn new(stream: S) -> Self
	where
		S: futures_util::TryStream<Ok = Event> + Send + 'static,
		S::Error: Into<axum::BoxError>,
	{
		Self(axum::response::sse::Sse::new(stream))
	}

	/// Configure the interval between keep-alive messages.
	///
	/// Defaults to no keep-alive messages.
	pub fn keep_alive(mut self, keep_alive: KeepAlive) -> Self {
		self.0 = self.0.keep_alive(keep_alive);

		self
	}
}

impl<S, E> IntoResponse for Sse<S>
where
	S: Stream<Item = Result<Event, E>> + Send + 'static,
	E: Into<axum::BoxError>,
{
	fn into_response(self) -> axum::response::Response {
		self.0.into_response()
	}
}

impl<S, E> aide::OperationOutput for Sse<S>
where
	S: Stream<Item = Result<Event, E>> + Send + 'static,
	E: Into<axum::BoxError>,
{
	type Inner = String;

	fn operation_response(ctx: &mut GenContext, _operation: &mut Operation) -> Option<Response> {
		Some(Response {
			description: "An SSE event stream".into(),
			content: IndexMap::from_iter([(
				"text/event-stream".into(),
				MediaType {
					schema: Some(SchemaObject {
						json_schema: String::json_schema(&mut ctx.schema),
						example: None,
						external_docs: None,
					}),
					..Default::default()
				},
			)]),
			..Default::default()
		})
	}

	fn inferred_responses(
		ctx: &mut aide::gen::GenContext,
		operation: &mut Operation,
	) -> Vec<(Option<u16>, Response)> {
		Self::operation_response(ctx, operation).map_or_else(Vec::new, |res| vec![(Some(200), res)])
	}
}

pub fn untar_to<R: Read>(mut tar: tar::Archive<R>, path: &Path) -> io::Result<()> {
	for entry in tar.entries()? {
		let mut file = entry?;
		let file_path = file.path()?.into_owned();
		let file_path = file_path
			.strip_prefix(file_path.components().next().unwrap())
			.unwrap()
			.to_owned();

		if file_path.to_str() == Some("") {
			continue;
		}

		if !file.header().entry_type().is_dir() {
			fs::create_dir_all(path.join(&file_path).parent().unwrap())?;
			file.unpack(path.join(file_path))?;
		}
	}

	Ok(())
}

pub fn spawn_with_logs(cmd: &mut Command) -> impl Stream<Item = io::Result<Log>> {
	let process = cmd.stdout(Stdio::piped()).stderr(Stdio::piped()).spawn();
	let pretty_cmd = pretty_cmd(cmd.as_std());

	try_fn_stream(|stream| async move {
		if let Some(pretty_cmd) = pretty_cmd {
			stream.emit(Log::Info(pretty_cmd)).await;
		}

		let mut process = process?;

		let mut stdout = BufReader::new(process.stdout.as_mut().unwrap()).lines();
		let mut stderr = BufReader::new(process.stderr.as_mut().unwrap()).lines();

		loop {
			tokio::select! {
				line = stdout.next_line() => {
					if let Some(line) = line? {
						stream.emit(Log::Info(line)).await;
					} else {
						break;
					}
				}
				line = stderr.next_line() => {
					if let Some(line) = line? {
						stream.emit(Log::Error(line)).await;
					} else {
						break;
					}
				}
			}
		}

		let status = process.wait().await?;
		if !status.success() {
			return Err(io::Error::new(
				io::ErrorKind::Other,
				format!("Command failed with status: {status}"),
			));
		}

		Ok(())
	})
}

fn pretty_cmd(cmd: &std::process::Command) -> Option<String> {
	let bin = cmd.get_program().to_str()?;
	let args = cmd
		.get_args()
		.map(|s| s.to_str())
		.collect::<Option<Rc<_>>>()?
		.join(" ");

	Some(format!("$ {bin} {args}"))
}
