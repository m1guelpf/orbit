use fern::colors::{Color, ColoredLevelConfig};
use log::{Level, LevelFilter};

pub fn set_hook() {
	// setup a panic hook to easily exit the program on panic
	std::panic::set_hook(Box::new(|panic_info| {
		// print the panic message
		let message = panic_info.payload().downcast_ref::<String>().map_or_else(
			|| {
				panic_info.payload().downcast_ref::<&str>().map_or_else(
					|| format!("{panic_info:?}"),
					|message| (*message).to_string(),
				)
			},
			Clone::clone,
		);

		// add some color
		log::error!("{message}");

		#[cfg(debug_assertions)]
		log::debug!("{panic_info}");

		std::process::exit(1);
	}));
}

pub fn logs(verbose: bool) {
	let colors = ColoredLevelConfig::new()
		.info(Color::BrightCyan)
		.error(Color::BrightRed)
		.warn(Color::BrightYellow)
		.debug(Color::BrightWhite);

	fern::Dispatch::new()
		.format(move |out, message, record| {
			let level = record.level();

			match level {
				Level::Debug => out.finish(format_args!(
					"{} [{}]: {}",
					colors.color(Level::Debug).to_string().to_lowercase(),
					record.target(),
					message
				)),

				level => out.finish(format_args!(
					"{}: {}",
					colors.color(level).to_string().to_lowercase(),
					message
				)),
			}
		})
		.level(if verbose {
			LevelFilter::Debug
		} else {
			LevelFilter::Info
		})
		.chain(
			fern::Dispatch::new()
				.filter(|metadata| !matches!(metadata.level(), Level::Error | Level::Warn))
				.chain(std::io::stdout()),
		)
		.chain(
			fern::Dispatch::new()
				.level(log::LevelFilter::Error)
				.level(log::LevelFilter::Warn)
				.chain(std::io::stderr()),
		)
		.apply()
		.ok();
}

pub fn clean_term() {
	let term = console::Term::stdout();

	// if the terminal is a tty, clear the screen and reset the cursor
	if term.is_term() {
		term.show_cursor().ok();
	}
}
