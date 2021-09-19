use std::{io::Stdout, sync::Arc};

use robespierre::{
    model::ChannelIdExt,
    robespierre_cache::{Cache, HasCache},
    robespierre_http::{HasHttp, Http},
    robespierre_models::{channels::Message, events::ServerToClientEvent, id::ChannelId},
};
use termion::{event::Key, input::MouseTerminal, raw::RawTerminal, screen::AlternateScreen};
use tui::{
    backend::TermionBackend,
    layout::{Constraint, Direction, Layout},
    style::{Color, Style},
    text::{Span, Spans},
    widgets::{Block, Borders, List, ListItem, Paragraph},
    Frame,
};
use unicode_width::UnicodeWidthStr;
use util::event::{Event, Events};

#[allow(dead_code)]
pub mod util;

type B = TermionBackend<AlternateScreen<MouseTerminal<RawTerminal<Stdout>>>>;

enum InputMode {
    Normal,
    Editing,
}

/// App holds the state of the application
pub struct AppState {
    /// Current value of the input box
    input: String,
    /// Current input mode
    input_mode: InputMode,
    /// History of recorded messages
    messages: Vec<Message>,

    current: ChannelId,

    cache: Arc<Cache>,
    http: Arc<Http>,
}

impl HasHttp for AppState {
    fn get_http(&self) -> &Http {
        &self.http
    }
}

impl HasCache for AppState {
    fn get_cache(&self) -> Option<&Cache> {
        Some(&*self.cache)
    }
}

impl AppState {
    pub fn new(current: ChannelId, cache: Arc<Cache>, http: Arc<Http>) -> Self {
        Self {
            input: String::new(),
            input_mode: InputMode::Normal,
            messages: Vec::new(),
            current,
            cache,
            http,
        }
    }
}

pub fn render(app: &AppState, f: &mut Frame<B>) {
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .margin(2)
        .constraints([Constraint::Length(3), Constraint::Min(1)].as_ref())
        .split(f.size());

    let input = Paragraph::new(app.input.as_ref())
        .style(match app.input_mode {
            InputMode::Normal => Style::default(),
            InputMode::Editing => Style::default().fg(Color::Yellow),
        })
        .block(Block::default().borders(Borders::ALL).title("Input"));
    f.render_widget(input, chunks[0]);
    match app.input_mode {
        InputMode::Normal =>
            // Hide the cursor. `Frame` does this by default, so we don't need to do anything here
            {}

        InputMode::Editing => {
            // Make the cursor visible and ask tui-rs to put it at the specified coordinates after rendering
            f.set_cursor(
                // Put cursor past the end of the input text
                chunks[0].x + app.input.width() as u16 + 1,
                // Move one line down, from the border to the input line
                chunks[0].y + 1,
            )
        }
    }

    let messages: Vec<ListItem> = app
        .messages
        .iter()
        .map(|message| {
            let content = vec![Spans::from(Span::raw(format!(
                "{}: {:?}",
                message.author, message.content
            )))];
            ListItem::new(content)
        })
        .collect();
    let messages = List::new(messages).block(Block::default().borders(Borders::ALL));
    f.render_widget(messages, chunks[1]);
}

pub enum Action {
    Break,
    None,
}

pub async fn update(app: &mut AppState, events: &mut Events) -> Action {
    // Handle input
    if let Some(ev) = events.next().await {
        match ev {
            Event::Input(input) => match app.input_mode {
                InputMode::Normal => match input {
                    Key::Char('e') => {
                        app.input_mode = InputMode::Editing;
                    }
                    Key::Char('q') => {
                        return Action::Break;
                    }
                    _ => {}
                },
                InputMode::Editing => match input {
                    Key::Char('\n') => {
                        let message = std::mem::take(&mut app.input);

                        let _ = app.current.send_message(app, |m| m.content(message)).await;
                    }
                    Key::Char(c) => {
                        app.input.push(c);
                    }
                    Key::Backspace => {
                        app.input.pop();
                    }
                    Key::Esc => {
                        app.input_mode = InputMode::Normal;
                    }
                    _ => {}
                },
            },
            Event::RobespierreEvent(ev) => {
                if let ServerToClientEvent::Message { message } = ev {
                    if app.current == message.channel {
                        app.messages.push(message);
                    }
                }
            }
            Event::Tick => {}
        }
    }

    Action::None
}
