use gpui::prelude::FluentBuilder;
use gpui::*;
use std::sync::mpsc;

pub enum PickerMode {
    List { items: Vec<String>, prompt: String },
    Freeform { prompt: String },
}

pub fn run_picker(mode: PickerMode) -> Option<String> {
    let (tx, rx) = mpsc::channel::<String>();

    let app = Application::new();
    app.run(move |cx: &mut App| {
        cx.bind_keys([
            KeyBinding::new("escape", Cancel, None),
            KeyBinding::new("enter", Confirm, None),
            KeyBinding::new("down", SelectNext, None),
            KeyBinding::new("up", SelectPrev, None),
            KeyBinding::new("tab", SelectNext, None),
        ]);

        let bounds = Bounds::centered(None, size(px(500.), px(420.)), cx);
        cx.open_window(
            WindowOptions {
                window_bounds: Some(WindowBounds::Windowed(bounds)),
                titlebar: Some(TitlebarOptions {
                    appears_transparent: true,
                    ..Default::default()
                }),
                ..Default::default()
            },
            move |window, cx| {
                let tx = tx.clone();
                cx.new(move |cx| PickerView::new(mode, tx, window, cx))
            },
        )
        .ok();
    });

    rx.try_recv().ok()
}

actions!(ws_picker, [Cancel, Confirm, SelectNext, SelectPrev]);

struct PickerView {
    items: Vec<String>,
    filtered: Vec<usize>,
    query: String,
    selected: usize,
    freeform: bool,
    prompt: String,
    tx: mpsc::Sender<String>,
    focus: FocusHandle,
}

impl PickerView {
    fn new(
        mode: PickerMode,
        tx: mpsc::Sender<String>,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Self {
        let (items, prompt, freeform) = match mode {
            PickerMode::List { items, prompt } => (items, prompt, false),
            PickerMode::Freeform { prompt } => (vec![], prompt, true),
        };
        let filtered = (0..items.len()).collect();
        let focus = cx.focus_handle();
        Self {
            items,
            filtered,
            query: String::new(),
            selected: 0,
            freeform,
            prompt,
            tx,
            focus,
        }
    }

    fn filter(&mut self) {
        let q = self.query.to_lowercase();
        if q.is_empty() {
            self.filtered = (0..self.items.len()).collect();
        } else {
            self.filtered = self
                .items
                .iter()
                .enumerate()
                .filter(|(_, item)| fuzzy_match(&item.to_lowercase(), &q))
                .map(|(i, _)| i)
                .collect();
        }
        self.selected = 0;
    }

    fn on_cancel(&mut self, _: &Cancel, _window: &mut Window, cx: &mut Context<Self>) {
        cx.quit();
    }

    fn on_confirm(&mut self, _: &Confirm, _window: &mut Window, cx: &mut Context<Self>) {
        let value = if self.freeform {
            if self.query.is_empty() {
                cx.quit();
                return;
            }
            self.query.clone()
        } else if let Some(&idx) = self.filtered.get(self.selected) {
            self.items[idx].clone()
        } else {
            cx.quit();
            return;
        };
        let _ = self.tx.send(value);
        cx.quit();
    }

    fn on_next(&mut self, _: &SelectNext, _window: &mut Window, cx: &mut Context<Self>) {
        if !self.filtered.is_empty() {
            self.selected = (self.selected + 1) % self.filtered.len();
            cx.notify();
        }
    }

    fn on_prev(&mut self, _: &SelectPrev, _window: &mut Window, cx: &mut Context<Self>) {
        if !self.filtered.is_empty() {
            self.selected = self
                .selected
                .checked_sub(1)
                .unwrap_or(self.filtered.len() - 1);
            cx.notify();
        }
    }
}

fn bg() -> Rgba { rgba(0x1e1e2eff) }
fn bg_hover() -> Rgba { rgba(0x313244ff) }
fn bg_selected() -> Rgba { rgba(0x45475aff) }
fn fg() -> Rgba { rgba(0xcdd6f4ff) }
fn accent() -> Rgba { rgba(0x89b4faff) }
fn active_dot() -> Rgba { rgba(0xa6e3a1ff) }
fn border_color() -> Rgba { rgba(0x313244ff) }

impl Render for PickerView {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let filtered = self.filtered.clone();
        let selected = self.selected;

        div()
            .key_context("Picker")
            .track_focus(&self.focus)
            .on_action(cx.listener(Self::on_cancel))
            .on_action(cx.listener(Self::on_confirm))
            .on_action(cx.listener(Self::on_next))
            .on_action(cx.listener(Self::on_prev))
            .flex()
            .flex_col()
            .size_full()
            .bg(bg())
            .text_color(fg())
            .font_family("monospace")
            .child(
                div()
                    .px(px(12.))
                    .py(px(10.))
                    .border_b_1()
                    .border_color(border_color())
                    .child(
                        div()
                            .flex()
                            .items_center()
                            .gap(px(8.))
                            .child(
                                div()
                                    .text_color(accent())
                                    .text_size(px(14.))
                                    .child(format!("{} ", self.prompt)),
                            )
                            .child(
                                div()
                                    .text_size(px(16.))
                                    .child(if self.query.is_empty() {
                                        "...".to_string()
                                    } else {
                                        self.query.clone()
                                    }),
                            ),
                    ),
            )
            .when(!self.freeform, |this: Div| {
                this.child(
                    div().flex().flex_col().overflow_y_hidden().children(
                        filtered.iter().enumerate().map(|(vi, &item_idx)| {
                            let item = self.items[item_idx].clone();
                            let is_selected = vi == selected;

                            let (dot, label) = if item.starts_with("● ") {
                                (true, item[4..].to_string())
                            } else if item.starts_with("  ") {
                                (false, item[2..].to_string())
                            } else {
                                (false, item.clone())
                            };

                            div()
                                .id(ElementId::Name(format!("item-{vi}").into()))
                                .px(px(16.))
                                .py(px(6.))
                                .bg(if is_selected { bg_selected() } else { bg() })
                                .hover(|s| s.bg(bg_hover()))
                                .cursor_pointer()
                                .on_mouse_down(
                                    MouseButton::Left,
                                    cx.listener(move |view, _, _, cx| {
                                        let _ = view.tx.send(view.items[item_idx].clone());
                                        cx.quit();
                                    }),
                                )
                                .child(
                                    div()
                                        .flex()
                                        .items_center()
                                        .gap(px(8.))
                                        .child(
                                            div()
                                                .w(px(8.))
                                                .h(px(8.))
                                                .rounded(px(4.))
                                                .when(dot, |s: Div| s.bg(active_dot()))
                                                .when(!dot, |s: Div| s.bg(gpui::transparent_black())),
                                        )
                                        .child(
                                            div()
                                                .text_size(px(14.))
                                                .when(is_selected, |s: Div| s.text_color(accent()))
                                                .child(label),
                                        ),
                                )
                        }),
                    ),
                )
            })
            .on_key_down(cx.listener(|view, event: &KeyDownEvent, _window, cx| {
                let key = &event.keystroke.key;
                if key.len() == 1 && !event.keystroke.modifiers.control {
                    let ch = key.chars().next().unwrap();
                    if ch.is_alphanumeric() || ch == '/' || ch == '-' || ch == '_' || ch == ' ' || ch == '.' {
                        view.query.push(ch);
                        view.filter();
                        cx.notify();
                    }
                } else if key == "backspace" {
                    view.query.pop();
                    view.filter();
                    cx.notify();
                }
            }))
    }
}

impl Focusable for PickerView {
    fn focus_handle(&self, _: &App) -> FocusHandle {
        self.focus.clone()
    }
}

fn fuzzy_match(text: &str, pattern: &str) -> bool {
    let mut pi = 0;
    let pattern_bytes = pattern.as_bytes();
    for &b in text.as_bytes() {
        if pi < pattern_bytes.len() && b == pattern_bytes[pi] {
            pi += 1;
        }
    }
    pi == pattern_bytes.len()
}
