use robespierre::{Authentication, robespierre_cache::{Cache, CacheConfig}, robespierre_http::Http};
use std::{error::Error, io, sync::Arc};
use termion::{input::MouseTerminal, raw::IntoRawMode, screen::AlternateScreen};
use tui::{backend::TermionBackend, Terminal};

use tui_revolt::{
    util::event::{Config, Events},
    Action, AppState,
};

async fn main_impl() -> Result<(), Box<dyn Error>> {
    let token = std::env::var("TOKEN")
        .expect("Cannot get token; set environment variable TOKEN=... and run again");

    let auth = Authentication::user(token);

    // Terminal initialization
    let stdout = io::stdout().into_raw_mode()?;
    let stdout = MouseTerminal::from(stdout);
    let stdout = AlternateScreen::from(stdout);
    let backend = TermionBackend::new(stdout);
    let mut terminal = Terminal::new(backend)?;

    // Setup event handlers and the robespierre connection
    let cache = Cache::new(CacheConfig::default());
    let mut events =
        Events::with_config(Config::new(auth.clone()), Arc::clone(&cache));

    let http = Arc::new(Http::new(&auth).await?);

    // Create new app state
    let mut app = AppState::new("01FEFZXHDQMD5ESK0XXW93JM5R".parse().unwrap(), cache, http);

    loop {
        // Draw UI
        terminal.draw(|f| tui_revolt::render(&app, f))?;

        match tui_revolt::update(&mut app, &mut events).await {
            Action::Break => break,
            Action::None => {}
        }
    }

    events.abort_tasks();

    Ok(())
}

fn main() -> Result<(), Box<dyn Error>> {
    let rt = tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .build()
        .unwrap();
    rt.block_on(main_impl())?;
    rt.shutdown_background(); // stop the tasks running in the background.

    Ok(())
}
