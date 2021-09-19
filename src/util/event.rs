use std::io;
use std::sync::Arc;
use std::time::Duration;

use robespierre::robespierre_cache::{Cache, CommitToCache};
use robespierre::robespierre_events::Connection;
use robespierre::robespierre_models::events::ServerToClientEvent;
use robespierre::Authentication;
use termion::event::Key;
use termion::input::TermRead;

use tokio::sync::mpsc::UnboundedReceiver;
use tokio::task::JoinHandle;

pub enum Event<I> {
    Input(I),
    RobespierreEvent(ServerToClientEvent),
    Tick,
}

/// A small event handler that wrap termion input and tick events. Each event
/// type is handled in its own thread and returned to a common `Receiver`
pub struct Events {
    rx: UnboundedReceiver<Event<Key>>,
    input_handle: JoinHandle<()>,
    tick_handle: JoinHandle<()>,
    robespierre_event_handle: JoinHandle<()>,
}

#[derive(Clone)]
pub struct Config {
    pub tick_rate: Duration,

    auth: Authentication,
}

impl Config {
    pub fn new(auth: Authentication) -> Self {
        Self {
            tick_rate: Duration::from_millis(250),
            auth,
        }
    }
}

impl Events {
    pub fn with_config(config: Config, cache: Arc<Cache>) -> Events {
        let (tx, rx) = tokio::sync::mpsc::unbounded_channel();
        let input_handle = {
            let tx = tx.clone();
            tokio::task::spawn_blocking(move || {
                let stdin = io::stdin();
                for evt in stdin.keys() {
                    if let Ok(key) = evt {
                        if let Err(err) = tx.send(Event::Input(key)) {
                            eprintln!("{}", err);
                            return;
                        }
                    }
                }
            })
        };
        let tick_handle = {
            let tx = tx.clone();
            let tick_rate = config.tick_rate;
            tokio::spawn(async move {
                loop {
                    if let Err(err) = tx.send(Event::Tick) {
                        eprintln!("{}", err);
                        break;
                    }
                    tokio::time::sleep(tick_rate).await;
                }
            })
        };

        let robespierre_event_handle = {
            tokio::spawn(async move {
                let auth = config.auth;

                let mut connection = match Connection::connect(&auth).await {
                    Ok(conn) => conn,
                    Err(e) => {
                        eprintln!("{}", e);
                        return;
                    }
                };

                loop {
                    let event = match connection.next().await {
                        Ok(ev) => ev,
                        Err(e) => {
                            eprintln!("{}", e);
                            break;
                        }
                    };

                    event.commit_to_cache_ref(&cache).await;

                    if let Err(err) = tx.send(Event::RobespierreEvent(event)) {
                        eprintln!("{}", err);
                        break;
                    }
                }
            })
        };

        Events {
            rx,
            input_handle,
            tick_handle,
            robespierre_event_handle,
        }
    }

    pub fn abort_tasks(&self) {
        self.input_handle.abort();
        self.tick_handle.abort();
        self.robespierre_event_handle.abort();
    }

    pub async fn next(&mut self) -> Option<Event<Key>> {
        self.rx.recv().await
    }
}
