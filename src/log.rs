use crate::theme;
use futures_channel::mpsc;
use gpui::*;
use std::sync::{Mutex, OnceLock};
use std::time::SystemTime;

const MAX_ENTRIES: usize = 2000;

#[derive(Clone)]
pub struct LogEntry {
    pub timestamp: SystemTime,
    pub message: String,
}

impl LogEntry {
    pub fn time_str(&self) -> String {
        let dur = self
            .timestamp
            .duration_since(SystemTime::UNIX_EPOCH)
            .unwrap_or_default();
        let secs = dur.as_secs() as libc::time_t;
        let mut tm: libc::tm = unsafe { std::mem::zeroed() };
        unsafe { libc::localtime_r(&secs, &mut tm) };
        format!(
            "{:02}:{:02}:{:02}",
            tm.tm_hour, tm.tm_min, tm.tm_sec
        )
    }
}

struct LogState {
    entries: Vec<LogEntry>,
    subscribers: Vec<mpsc::UnboundedSender<LogEntry>>,
}

static LOG: OnceLock<Mutex<LogState>> = OnceLock::new();

fn state() -> &'static Mutex<LogState> {
    LOG.get_or_init(|| {
        Mutex::new(LogState {
            entries: Vec::new(),
            subscribers: Vec::new(),
        })
    })
}

pub fn log(msg: impl Into<String>) {
    let entry = LogEntry {
        timestamp: SystemTime::now(),
        message: msg.into(),
    };
    eprintln!("[log] {}", entry.message);
    let mut st = state().lock().unwrap();
    st.entries.push(entry.clone());
    if st.entries.len() > MAX_ENTRIES {
        let drain = st.entries.len() - MAX_ENTRIES;
        st.entries.drain(..drain);
    }
    st.subscribers.retain(|tx| tx.unbounded_send(entry.clone()).is_ok());
}

pub fn subscribe() -> (Vec<LogEntry>, mpsc::UnboundedReceiver<LogEntry>) {
    let (tx, rx) = mpsc::unbounded();
    let mut st = state().lock().unwrap();
    let snapshot = st.entries.clone();
    st.subscribers.push(tx);
    (snapshot, rx)
}

static LOG_WINDOW_TX: OnceLock<mpsc::UnboundedSender<()>> = OnceLock::new();

pub fn setup_log_listener(cx: &mut App) -> mpsc::UnboundedReceiver<()> {
    let (tx, rx) = mpsc::unbounded::<()>();
    let _ = LOG_WINDOW_TX.set(tx);
    cx.bind_keys([KeyBinding::new("escape", DismissLog, None)]);
    rx
}

pub fn request_log_window() {
    if let Some(tx) = LOG_WINDOW_TX.get() {
        let _ = tx.unbounded_send(());
    }
}

fn bg() -> Rgba { theme::bg() }
fn fg() -> Rgba { theme::fg() }
fn fg_dim() -> Rgba { theme::fg_dim() }
fn border_color() -> Rgba { theme::border_color() }
fn accent() -> Rgba { theme::accent() }

actions!(log_viewer, [DismissLog]);

struct LogView {
    entries: Vec<LogEntry>,
    focus: FocusHandle,
    scroll_handle: ScrollHandle,
    _subscription_task: Task<()>,
}

pub fn show_log_window(cx: &mut App) {
    let (snapshot, rx) = subscribe();

    let bounds = Bounds::centered(None, size(px(750.), px(500.)), cx);
    cx.open_window(
        WindowOptions {
            window_bounds: Some(WindowBounds::Windowed(bounds)),
            titlebar: None,
            app_id: Some("awesometree-log".into()),
            window_decorations: Some(WindowDecorations::Server),
            ..Default::default()
        },
        move |_window, cx| {
            cx.new(move |cx| LogView::new(snapshot, rx, cx))
        },
    )
    .ok();
}

impl LogView {
    fn new(
        entries: Vec<LogEntry>,
        mut rx: mpsc::UnboundedReceiver<LogEntry>,
        cx: &mut Context<Self>,
    ) -> Self {
        let task = cx.spawn(async move |this: WeakEntity<Self>, cx: &mut AsyncApp| {
            use futures_util::StreamExt;
            while let Some(entry) = rx.next().await {
                let _ = cx.update(|cx| {
                    let _ = this.update(cx, |view, cx| {
                        view.entries.push(entry);
                        if view.entries.len() > MAX_ENTRIES {
                            let drain = view.entries.len() - MAX_ENTRIES;
                            view.entries.drain(..drain);
                        }
                        cx.notify();
                    });
                });
            }
        });

        Self {
            entries,
            focus: cx.focus_handle(),
            scroll_handle: ScrollHandle::new(),
            _subscription_task: task,
        }
    }

    fn on_dismiss(&mut self, _: &DismissLog, window: &mut Window, _cx: &mut Context<Self>) {
        window.remove_window();
    }
}

impl Render for LogView {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let count = self.entries.len();

        div()
            .key_context("LogViewer")
            .track_focus(&self.focus)
            .on_action(cx.listener(Self::on_dismiss))
            .flex()
            .flex_col()
            .size_full()
            .bg(bg())
            .text_color(fg())
            .font_family("monospace")
            .child(
                div()
                    .px(px(20.))
                    .py(px(14.))
                    .border_b_1()
                    .border_color(border_color())
                    .flex()
                    .justify_between()
                    .items_center()
                    .child(
                        div()
                            .text_size(px(18.))
                            .text_color(accent())
                            .child("Daemon Log"),
                    )
                    .child(
                        div()
                            .text_size(px(12.))
                            .text_color(fg_dim())
                            .child(format!("{count} entries")),
                    ),
            )
            .child(
                div()
                    .id("log-scroll")
                    .flex_1()
                    .overflow_y_hidden()
                    .track_scroll(&self.scroll_handle)
                    .child(
                        div().flex().flex_col().children(
                            self.entries.iter().enumerate().map(|(i, entry)| {
                                let ts = entry.time_str();
                                let msg = entry.message.clone();
                                div()
                                    .id(ElementId::Integer(i as u64))
                                    .px(px(16.))
                                    .py(px(3.))
                                    .flex()
                                    .gap(px(10.))
                                    .child(
                                        div()
                                            .text_size(px(12.))
                                            .text_color(fg_dim())
                                            .min_w(px(60.))
                                            .child(ts),
                                    )
                                    .child(
                                        div()
                                            .text_size(px(13.))
                                            .text_color(fg())
                                            .child(msg),
                                    )
                            }),
                        ),
                    ),
            )
            .child(
                div()
                    .px(px(20.))
                    .py(px(10.))
                    .border_t_1()
                    .border_color(border_color())
                    .child(
                        div()
                            .text_size(px(11.))
                            .text_color(fg_dim())
                            .child("Esc to close"),
                    ),
            )
    }
}

impl Focusable for LogView {
    fn focus_handle(&self, _: &App) -> FocusHandle {
        self.focus.clone()
    }
}
