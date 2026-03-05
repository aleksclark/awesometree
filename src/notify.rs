use futures_channel::mpsc;
use gpui::*;
use std::sync::OnceLock;
use std::thread;

static ERROR_TX: OnceLock<mpsc::UnboundedSender<String>> = OnceLock::new();

fn bg() -> Rgba { rgba(0x1e1e2eff) }
fn fg() -> Rgba { rgba(0xcdd6f4ff) }
fn fg_dim() -> Rgba { rgba(0x6c7086ff) }
fn border_color() -> Rgba { rgba(0x313244ff) }
fn danger() -> Rgba { rgba(0xf38ba8ff) }
fn btn_bg() -> Rgba { rgba(0x89b4faff) }
fn btn_fg() -> Rgba { rgba(0x1e1e2eff) }
fn btn_hover() -> Rgba { rgba(0xb4d0fbff) }

actions!(notify, [DismissError]);

pub fn open_sentinel_window(cx: &mut App) {
    let bounds = Bounds::new(point(px(-100.), px(-100.)), size(px(1.), px(1.)));
    cx.open_window(
        WindowOptions {
            window_bounds: Some(WindowBounds::Windowed(bounds)),
            titlebar: None,
            app_id: Some("awesometree-sentinel".into()),
            window_decorations: Some(WindowDecorations::Server),
            ..Default::default()
        },
        |_window, cx| cx.new(|_cx| SentinelView),
    )
    .ok();
}

struct SentinelView;

impl Render for SentinelView {
    fn render(&mut self, _window: &mut Window, _cx: &mut Context<Self>) -> impl IntoElement {
        div().size(px(1.))
    }
}

impl Focusable for SentinelView {
    fn focus_handle(&self, _: &App) -> FocusHandle {
        unreachable!()
    }
}

pub fn setup_error_listener(cx: &mut App) -> mpsc::UnboundedReceiver<String> {
    let (tx, rx) = mpsc::unbounded::<String>();
    let _ = ERROR_TX.set(tx);
    cx.bind_keys([KeyBinding::new("escape", DismissError, None)]);
    rx
}

pub fn report_error(msg: impl Into<String>) {
    let msg = msg.into();
    eprintln!("awesometree error: {msg}");
    if let Some(tx) = ERROR_TX.get() {
        let _ = tx.unbounded_send(msg);
    }
}

pub fn show_error_window(cx: &mut App, message: String) {
    let bounds = Bounds::centered(None, size(px(500.), px(250.)), cx);
    cx.open_window(
        WindowOptions {
            window_bounds: Some(WindowBounds::Windowed(bounds)),
            titlebar: None,
            app_id: Some("awesometree-error".into()),
            window_decorations: Some(WindowDecorations::Server),
            ..Default::default()
        },
        move |_window, cx| cx.new(move |cx| ErrorView::new(message, cx)),
    )
    .ok();
}

struct ErrorView {
    message: String,
    focus: FocusHandle,
}

impl ErrorView {
    fn new(message: String, cx: &mut Context<Self>) -> Self {
        Self {
            message,
            focus: cx.focus_handle(),
        }
    }

    fn on_dismiss(&mut self, _: &DismissError, window: &mut Window, _cx: &mut Context<Self>) {
        window.remove_window();
    }
}

impl Render for ErrorView {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let msg = self.message.clone();

        div()
            .key_context("Error")
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
                    .items_center()
                    .gap(px(10.))
                    .child(
                        div()
                            .text_size(px(18.))
                            .text_color(danger())
                            .child("Error"),
                    ),
            )
            .child(
                div()
                    .flex_1()
                    .px(px(20.))
                    .py(px(16.))
                    .child(
                        div()
                            .text_size(px(14.))
                            .text_color(fg())
                            .child(msg),
                    ),
            )
            .child(
                div()
                    .px(px(20.))
                    .py(px(12.))
                    .border_t_1()
                    .border_color(border_color())
                    .flex()
                    .justify_between()
                    .items_center()
                    .child(
                        div()
                            .text_size(px(11.))
                            .text_color(fg_dim())
                            .child("Press Esc or click OK to dismiss"),
                    )
                    .child(
                        div()
                            .id("ok-btn")
                            .px(px(24.))
                            .py(px(6.))
                            .rounded(px(4.))
                            .bg(btn_bg())
                            .text_color(btn_fg())
                            .cursor_pointer()
                            .hover(|s| s.bg(btn_hover()))
                            .on_mouse_down(
                                MouseButton::Left,
                                cx.listener(|_view, _, window, _cx| {
                                    window.remove_window();
                                }),
                            )
                            .child(div().text_size(px(13.)).child("OK")),
                    ),
            )
    }
}

impl Focusable for ErrorView {
    fn focus_handle(&self, _: &App) -> FocusHandle {
        self.focus.clone()
    }
}

pub fn spawn_task<F>(label: &str, f: F)
where
    F: FnOnce() -> Result<(), String> + Send + 'static,
{
    let label = label.to_string();
    thread::spawn(move || {
        let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(f));
        match result {
            Ok(Ok(())) => {}
            Ok(Err(e)) => report_error(format!("{label}: {e}")),
            Err(panic) => {
                let msg = if let Some(s) = panic.downcast_ref::<&str>() {
                    s.to_string()
                } else if let Some(s) = panic.downcast_ref::<String>() {
                    s.clone()
                } else {
                    "unknown panic".to_string()
                };
                report_error(format!("{label} crashed: {msg}"));
            }
        }
    });
}

static PROGRESS_TX: OnceLock<mpsc::UnboundedSender<ProgressMsg>> = OnceLock::new();

pub enum ProgressMsg {
    Open { title: String },
    Update(String),
    Done,
    Error(String),
}

pub struct ProgressHandle;

impl ProgressHandle {
    pub fn update(&self, msg: impl Into<String>) {
        if let Some(tx) = PROGRESS_TX.get() {
            let _ = tx.unbounded_send(ProgressMsg::Update(msg.into()));
        }
    }

    pub fn done(self) {
        if let Some(tx) = PROGRESS_TX.get() {
            let _ = tx.unbounded_send(ProgressMsg::Done);
        }
    }

    pub fn error(self, msg: impl Into<String>) {
        if let Some(tx) = PROGRESS_TX.get() {
            let _ = tx.unbounded_send(ProgressMsg::Error(msg.into()));
        }
    }
}

pub fn open_progress(title: impl Into<String>) -> ProgressHandle {
    if let Some(tx) = PROGRESS_TX.get() {
        let _ = tx.unbounded_send(ProgressMsg::Open {
            title: title.into(),
        });
    }
    ProgressHandle
}

pub fn setup_progress_listener(_cx: &mut App) -> mpsc::UnboundedReceiver<ProgressMsg> {
    let (tx, rx) = mpsc::unbounded::<ProgressMsg>();
    let _ = PROGRESS_TX.set(tx);
    rx
}

pub struct ProgressView {
    title: String,
    status: String,
    focus: FocusHandle,
}

impl ProgressView {
    fn new(title: String, cx: &mut Context<Self>) -> Self {
        Self {
            title,
            status: String::new(),
            focus: cx.focus_handle(),
        }
    }
}

impl Render for ProgressView {
    fn render(&mut self, _window: &mut Window, _cx: &mut Context<Self>) -> impl IntoElement {
        div()
            .key_context("Progress")
            .track_focus(&self.focus)
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
                    .child(
                        div()
                            .text_size(px(16.))
                            .text_color(rgba(0x89b4faff))
                            .child(self.title.clone()),
                    ),
            )
            .child(
                div()
                    .flex_1()
                    .px(px(20.))
                    .py(px(16.))
                    .child(
                        div()
                            .text_size(px(13.))
                            .text_color(fg_dim())
                            .child(if self.status.is_empty() {
                                "Starting...".to_string()
                            } else {
                                self.status.clone()
                            }),
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
                            .child("Please wait..."),
                    ),
            )
    }
}

impl Focusable for ProgressView {
    fn focus_handle(&self, _: &App) -> FocusHandle {
        self.focus.clone()
    }
}

pub fn show_progress_window(cx: &mut App, title: String) -> Option<WindowHandle<ProgressView>> {
    let bounds = Bounds::centered(None, size(px(450.), px(180.)), cx);
    cx.open_window(
        WindowOptions {
            window_bounds: Some(WindowBounds::Windowed(bounds)),
            titlebar: None,
            app_id: Some("awesometree-progress".into()),
            window_decorations: Some(WindowDecorations::Server),
            ..Default::default()
        },
        move |_window, cx| cx.new(move |cx| ProgressView::new(title, cx)),
    )
    .ok()
}

pub fn update_progress_window(
    handle: &WindowHandle<ProgressView>,
    status: String,
    cx: &mut App,
) {
    let _ = handle.update(cx, |view: &mut ProgressView, _window, cx| {
        view.status = status;
        cx.notify();
    });
}

pub fn close_progress_window(handle: &WindowHandle<ProgressView>, cx: &mut App) {
    let _ = handle.update(cx, |_view: &mut ProgressView, window, _cx| {
        window.remove_window();
    });
}
