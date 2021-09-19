use std::{
    convert::{TryFrom, TryInto},
    io::Stdout,
    sync::Arc,
};

use robespierre::{
    model::{user_opt_member::UserOptMember, ChannelIdExt, MessageExt, ServerIdExt},
    robespierre_cache::{Cache, HasCache},
    robespierre_http::{HasHttp, Http},
    robespierre_models::{
        channels::{Channel, Message},
        events::ServerToClientEvent,
        id::ChannelId,
        servers::Server,
    },
};
use termion::{event::Key, input::MouseTerminal, raw::RawTerminal, screen::AlternateScreen};
use tui::{
    backend::TermionBackend,
    layout::{Constraint, Corner, Direction, Layout},
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

enum AppStateInternal {
    ServerChannel {
        /// Current value of the input box
        input: String,
        /// Current input mode
        input_mode: InputMode,

        /// History of recorded messages
        messages: Vec<(Message, UserOptMember)>,

        server: Server,

        server_channels: Vec<Channel>,

        current_channel: Channel,
    },
}

/// App holds the state of the application
pub struct AppState {
    state: AppStateInternal,
    server_list: Option<Vec<Server>>,

    ctx: AppCtx,
}

struct AppCtx {
    cache: Arc<Cache>,
    http: Arc<Http>,
}

impl HasHttp for AppCtx {
    fn get_http(&self) -> &Http {
        &self.http
    }
}

impl HasCache for AppCtx {
    fn get_cache(&self) -> Option<&Cache> {
        Some(&*self.cache)
    }
}

pub enum OpenAt {
    Channel(ChannelId),
}

impl AppState {
    pub async fn new(
        cache: Arc<Cache>,
        http: Arc<Http>,
        open_at: OpenAt,
    ) -> robespierre::Result<Self> {
        let ctx = AppCtx { cache, http };
        let state = match open_at {
            OpenAt::Channel(channel_id) => {
                let current_channel = channel_id.channel(&ctx).await?;

                let server_id = current_channel.server_id();

                match server_id {
                    Some(server_id) => {
                        let server = server_id.server(&ctx).await?;
                        let mut server_channels = Vec::with_capacity(server.channels.len());

                        for ch in server.channels.iter() {
                            server_channels.push(ch.channel(&ctx).await?);
                        }

                        AppStateInternal::ServerChannel {
                            input: String::new(),
                            input_mode: InputMode::Normal,
                            messages: Vec::new(),
                            current_channel,
                            server,
                            server_channels,
                        }
                    }
                    None => {
                        todo!()
                    }
                }
            }
        };

        Ok(Self {
            state,
            ctx,
            server_list: None,
        })
    }
}

pub trait ToArray {
    type Item: 'static;
    fn to_array<const N: usize>(self) -> [Self::Item; N]
    where
        for<'a> [Self::Item; N]: TryFrom<&'a [Self::Item]>;
}

impl<T> ToArray for Vec<T>
where
    T: 'static,
{
    type Item = T;

    fn to_array<const N: usize>(self) -> [Self::Item; N]
    where
        for<'a> [Self::Item; N]: TryFrom<&'a [Self::Item]>,
    {
        match self[..].try_into() {
            Ok(t) => t,
            Err(_) => panic!("incorrect length"),
        }
    }
}

pub fn render(app: &AppState, f: &mut Frame<B>) {
    let [server_list_container, main_container] = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([Constraint::Percentage(5), Constraint::Percentage(95)].as_ref())
        .split(f.size())
        .to_array();

    let servers: Vec<ListItem> = app.server_list.as_ref().map_or_else(
        || vec![],
        |server_list| {
            server_list
                .iter()
                .map(|server| {
                    let content = vec![Spans::from(Span::raw(&server.name))];
                    ListItem::new(content)
                })
                .collect()
        },
    );
    let servers = List::new(servers).block(Block::default().borders(Borders::ALL));
    f.render_widget(servers, server_list_container);

    match &app.state {
        AppStateInternal::ServerChannel {
            input,
            input_mode,
            messages,
            current_channel,
            server,
            server_channels,
        } => {
            let [server_bar, inner_container, _members_list] = Layout::default()
                .direction(Direction::Horizontal)
                .constraints(
                    [
                        Constraint::Percentage(10),
                        Constraint::Percentage(80),
                        Constraint::Percentage(10),
                    ]
                    .as_ref(),
                )
                .split(main_container)
                .to_array();

            let [server_bar_header, channels_list_container] = Layout::default()
                .direction(Direction::Vertical)
                .constraints([Constraint::Percentage(20), Constraint::Percentage(80)].as_ref())
                .split(server_bar)
                .to_array();

            let server_bar_p = Paragraph::new(
                server
                    .description
                    .as_ref()
                    .map(|it| it.as_str())
                    .unwrap_or("Dummy server description"),
            )
            .style(Style::default().fg(Color::LightCyan))
            .block(
                Block::default()
                    .borders(Borders::BOTTOM | Borders::LEFT | Borders::RIGHT)
                    .title(server.name.as_str()),
            );
            f.render_widget(server_bar_p, server_bar_header);

            let channels: Vec<ListItem> = server_channels
                .iter()
                .map(|channel| {
                    let content = vec![Spans::from(Span::raw(channel.name().unwrap().clone()))];
                    ListItem::new(content)
                })
                .collect();
            let channels = List::new(channels).block(Block::default().borders(Borders::ALL));
            f.render_widget(channels, channels_list_container);

            let [channel_header, messages_container, input_container] = Layout::default()
                .direction(Direction::Vertical)
                .constraints(
                    [
                        Constraint::Length(3),
                        Constraint::Min(3),
                        Constraint::Length(3),
                    ]
                    .as_ref(),
                )
                .split(inner_container)
                .to_array();

            let channel_desc_p = Paragraph::new(
                current_channel
                    .description()
                    .map(|it| it.as_str())
                    .unwrap_or("Dummy channel description"),
            )
            .style(Style::default().fg(Color::Blue))
            .block(
                Block::default()
                    .borders(Borders::BOTTOM | Borders::LEFT | Borders::RIGHT)
                    .title(
                        current_channel
                            .name()
                            .map(|it| it.as_str())
                            .unwrap_or("Dummy channel name"),
                    ),
            );
            f.render_widget(channel_desc_p, channel_header);

            let messages: Vec<ListItem> = messages
                .iter()
                .rev()
                .map(|message| {
                    let content = vec![Spans::from(Span::raw(format!(
                        "{}: {:?}",
                        message.1.display_name(),
                        message.0.content
                    )))];
                    ListItem::new(content)
                })
                .collect();
            let messages = List::new(messages)
                .block(Block::default().borders(Borders::ALL))
                .start_corner(Corner::BottomLeft);
            f.render_widget(messages, messages_container);

            let input_p = Paragraph::new(input.as_ref())
                .style(match input_mode {
                    InputMode::Normal => Style::default(),
                    InputMode::Editing => Style::default().fg(Color::Yellow),
                })
                .block(Block::default().borders(Borders::ALL).title("Input"));
            f.render_widget(input_p, input_container);
            match input_mode {
                InputMode::Normal =>
                    // Hide the cursor. `Frame` does this by default, so we don't need to do anything here
                    {}

                InputMode::Editing => {
                    // Make the cursor visible and ask tui-rs to put it at the specified coordinates after rendering
                    f.set_cursor(
                        // Put cursor past the end of the input text
                        input_container.x + input.width() as u16 + 1,
                        // Move one line down, from the border to the input line
                        input_container.y + 1,
                    )
                }
            }
        }
    }
}

pub enum Action {
    Break,
    None,
}

pub async fn update(app: &mut AppState, events: &mut Events) -> Action {
    // Handle input
    if let Some(ev) = events.next().await {
        let AppState {
            state,
            ctx,
            server_list,
        } = app;

        match state {
            AppStateInternal::ServerChannel {
                input,
                input_mode,
                messages,
                current_channel: current,
                server: _,
                server_channels: _,
            } => match ev {
                Event::Input(input_key) => match input_mode {
                    InputMode::Normal => match input_key {
                        Key::Char('e') => {
                            *input_mode = InputMode::Editing;
                        }
                        Key::Char('q') => {
                            return Action::Break;
                        }
                        _ => {}
                    },
                    InputMode::Editing => match input_key {
                        Key::Char('\n') => {
                            let message = std::mem::take(input);

                            let _ = current.id().send_message(ctx, |m| m.content(message)).await;
                        }
                        Key::Char(c) => {
                            input.push(c);
                        }
                        Key::Backspace => {
                            input.pop();
                        }
                        Key::Esc => {
                            *input_mode = InputMode::Normal;
                        }
                        _ => {}
                    },
                },
                Event::RobespierreEvent(ev) => match ev {
                    ServerToClientEvent::Message { message } => {
                        if current.id() == message.channel {
                            let user_opt_member =
                                message.author_user_opt_member(ctx).await.unwrap();
                            messages.push((message, user_opt_member));
                        }
                    }
                    ServerToClientEvent::Ready { event } => {
                        *server_list = Some(event.servers);
                    }
                    _ => {}
                },
                Event::Tick => {}
            },
        }
    }

    Action::None
}
