use crate::config::{self, Config, Project};
use gpui::prelude::FluentBuilder;
use gpui::*;

pub fn open_projects_window(cx: &mut App) {
    let cfg = config::load_config().unwrap_or_default();

    let bounds = Bounds::centered(None, size(px(700.), px(500.)), cx);
    cx.open_window(
        WindowOptions {
            window_bounds: Some(WindowBounds::Windowed(bounds)),
            titlebar: None,
            app_id: Some("awesometree-projects".into()),
            window_decorations: Some(WindowDecorations::Server),
            ..Default::default()
        },
        move |_window, cx| cx.new(move |cx| ProjectsView::new(cfg, cx)),
    )
    .ok();
}

pub fn run_projects_ui() {
    let cfg = config::load_config().unwrap_or_default();

    let app = Application::new();
    app.run(move |cx: &mut App| {
        cx.bind_keys([
            KeyBinding::new("escape", Dismiss, None),
            KeyBinding::new("enter", ConfirmAction, None),
            KeyBinding::new("tab", NextField, None),
            KeyBinding::new("shift-tab", PrevField, None),
        ]);

        let bounds = Bounds::centered(None, size(px(700.), px(500.)), cx);
        cx.open_window(
            WindowOptions {
                window_bounds: Some(WindowBounds::Windowed(bounds)),
                titlebar: None,
                app_id: Some("awesometree-projects".into()),
                window_decorations: Some(WindowDecorations::Server),
                ..Default::default()
            },
            move |_window, cx| cx.new(move |cx| ProjectsView::new(cfg, cx)),
        )
        .ok();
    });
}

actions!(projects_ui, [Dismiss, ConfirmAction, NextField, PrevField]);

fn bg() -> Rgba { rgba(0x1e1e2eff) }
fn bg_hover() -> Rgba { rgba(0x313244ff) }
fn bg_selected() -> Rgba { rgba(0x45475aff) }
fn fg() -> Rgba { rgba(0xcdd6f4ff) }
fn fg_dim() -> Rgba { rgba(0x6c7086ff) }
fn accent() -> Rgba { rgba(0x89b4faff) }
fn border_color() -> Rgba { rgba(0x313244ff) }
fn border_focus() -> Rgba { rgba(0x89b4faff) }
fn btn_bg() -> Rgba { rgba(0x89b4faff) }
fn btn_fg() -> Rgba { rgba(0x1e1e2eff) }
fn btn_hover() -> Rgba { rgba(0xb4d0fbff) }
fn danger() -> Rgba { rgba(0xf38ba8ff) }
fn success() -> Rgba { rgba(0xa6e3a1ff) }

#[derive(Clone, Copy, PartialEq)]
enum Mode {
    List,
    Adding,
    Editing(usize),
}

#[derive(Clone, Copy, PartialEq)]
enum FormField {
    Name,
    Repo,
    Branch,
}

struct ProjectsView {
    config: Config,
    mode: Mode,
    form_name: String,
    form_repo: String,
    form_branch: String,
    form_field: FormField,
    focus: FocusHandle,
}

impl ProjectsView {
    fn new(config: Config, cx: &mut Context<Self>) -> Self {
        Self {
            config,
            mode: Mode::List,
            form_name: String::new(),
            form_repo: String::new(),
            form_branch: String::new(),
            form_field: FormField::Name,
            focus: cx.focus_handle(),
        }
    }

    fn clear_form(&mut self) {
        self.form_name.clear();
        self.form_repo.clear();
        self.form_branch.clear();
        self.form_field = FormField::Name;
    }

    fn start_add(&mut self) {
        self.clear_form();
        self.mode = Mode::Adding;
    }

    fn start_edit(&mut self, idx: usize) {
        let p = &self.config.projects[idx];
        self.form_name = p.name.clone();
        self.form_repo = p.repo.clone();
        self.form_branch = p.branch.clone();
        self.form_field = FormField::Name;
        self.mode = Mode::Editing(idx);
    }

    fn save_form(&mut self) {
        match self.mode {
            Mode::Adding => {
                if self.form_name.is_empty() || self.form_repo.is_empty() {
                    return;
                }
                let branch = if self.form_branch.is_empty() {
                    "master".to_string()
                } else {
                    self.form_branch.clone()
                };
                self.config.add_project(&self.form_name, &self.form_repo, &branch);
                let _ = config::save_config(&self.config);
                self.mode = Mode::List;
                self.clear_form();
            }
            Mode::Editing(idx) => {
                if self.form_name.is_empty() || self.form_repo.is_empty() {
                    return;
                }
                let p = &mut self.config.projects[idx];
                p.name = self.form_name.clone();
                p.repo = self.form_repo.clone();
                p.branch = if self.form_branch.is_empty() {
                    "master".to_string()
                } else {
                    self.form_branch.clone()
                };
                let _ = config::save_config(&self.config);
                self.mode = Mode::List;
                self.clear_form();
            }
            Mode::List => {}
        }
    }

    fn delete_project(&mut self, idx: usize) {
        self.config.projects.remove(idx);
        let _ = config::save_config(&self.config);
        if let Mode::Editing(ei) = self.mode {
            if ei == idx {
                self.mode = Mode::List;
                self.clear_form();
            }
        }
    }

    fn push_char(&mut self, ch: char) {
        match self.form_field {
            FormField::Name => self.form_name.push(ch),
            FormField::Repo => self.form_repo.push(ch),
            FormField::Branch => self.form_branch.push(ch),
        }
    }

    fn pop_char(&mut self) {
        match self.form_field {
            FormField::Name => { self.form_name.pop(); }
            FormField::Repo => { self.form_repo.pop(); }
            FormField::Branch => { self.form_branch.pop(); }
        }
    }

    fn on_dismiss(&mut self, _: &Dismiss, window: &mut Window, cx: &mut Context<Self>) {
        match self.mode {
            Mode::List => window.remove_window(),
            _ => {
                self.mode = Mode::List;
                self.clear_form();
                cx.notify();
            }
        }
    }

    fn on_confirm(&mut self, _: &ConfirmAction, _window: &mut Window, cx: &mut Context<Self>) {
        if self.mode != Mode::List {
            self.save_form();
            cx.notify();
        }
    }

    fn on_next_field(&mut self, _: &NextField, _window: &mut Window, cx: &mut Context<Self>) {
        if self.mode != Mode::List {
            self.form_field = match self.form_field {
                FormField::Name => FormField::Repo,
                FormField::Repo => FormField::Branch,
                FormField::Branch => FormField::Name,
            };
            cx.notify();
        }
    }

    fn on_prev_field(&mut self, _: &PrevField, _window: &mut Window, cx: &mut Context<Self>) {
        if self.mode != Mode::List {
            self.form_field = match self.form_field {
                FormField::Name => FormField::Branch,
                FormField::Repo => FormField::Name,
                FormField::Branch => FormField::Repo,
            };
            cx.notify();
        }
    }
}

fn render_field(label: &str, value: &str, focused: bool, placeholder: &str) -> Div {
    div()
        .flex()
        .flex_col()
        .gap(px(4.))
        .child(
            div()
                .text_size(px(12.))
                .text_color(if focused { accent() } else { fg_dim() })
                .child(label.to_string()),
        )
        .child(
            div()
                .px(px(10.))
                .py(px(6.))
                .rounded(px(4.))
                .border_1()
                .border_color(if focused { border_focus() } else { border_color() })
                .bg(bg_hover())
                .child(
                    div()
                        .text_size(px(14.))
                        .text_color(if value.is_empty() { fg_dim() } else { fg() })
                        .child(if value.is_empty() {
                            placeholder.to_string()
                        } else if focused {
                            format!("{value}_")
                        } else {
                            value.to_string()
                        }),
                ),
        )
}

impl Render for ProjectsView {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let projects: Vec<(usize, Project)> = self
            .config
            .projects
            .iter()
            .enumerate()
            .map(|(i, p)| (i, p.clone()))
            .collect();

        let mode = self.mode;
        let form_name = self.form_name.clone();
        let form_repo = self.form_repo.clone();
        let form_branch = self.form_branch.clone();
        let field = self.form_field;
        let can_save = !form_name.is_empty() && !form_repo.is_empty();

        div()
            .key_context("Projects")
            .track_focus(&self.focus)
            .on_action(cx.listener(Self::on_dismiss))
            .on_action(cx.listener(Self::on_confirm))
            .on_action(cx.listener(Self::on_next_field))
            .on_action(cx.listener(Self::on_prev_field))
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
                            .child("Projects"),
                    )
                    .when(mode == Mode::List, |this: Div| {
                        this.child(
                            div()
                                .id("add-btn")
                                .px(px(16.))
                                .py(px(6.))
                                .rounded(px(4.))
                                .bg(btn_bg())
                                .text_color(btn_fg())
                                .cursor_pointer()
                                .hover(|s| s.bg(btn_hover()))
                                .on_mouse_down(
                                    MouseButton::Left,
                                    cx.listener(|view, _, _, cx| {
                                        view.start_add();
                                        cx.notify();
                                    }),
                                )
                                .child(div().text_size(px(13.)).child("+ Add Project")),
                        )
                    }),
            )
            .child(
                div()
                    .flex()
                    .flex_col()
                    .flex_1()
                    .overflow_y_hidden()
                    .when(mode != Mode::List, |this: Div| {
                        let title = match mode {
                            Mode::Adding => "New Project",
                            Mode::Editing(_) => "Edit Project",
                            _ => "",
                        };
                        this.child(
                            div()
                                .px(px(20.))
                                .py(px(16.))
                                .flex()
                                .flex_col()
                                .gap(px(14.))
                                .child(
                                    div()
                                        .text_size(px(15.))
                                        .text_color(fg())
                                        .child(title.to_string()),
                                )
                                .child(render_field(
                                    "NAME",
                                    &form_name,
                                    field == FormField::Name,
                                    "e.g. curri",
                                ))
                                .child(render_field(
                                    "REPO PATH",
                                    &form_repo,
                                    field == FormField::Repo,
                                    "/path/to/git/repo",
                                ))
                                .child(render_field(
                                    "SOURCE BRANCH",
                                    &form_branch,
                                    field == FormField::Branch,
                                    "master",
                                ))
                                .child(
                                    div()
                                        .flex()
                                        .gap(px(10.))
                                        .child(
                                            div()
                                                .id("save-btn")
                                                .px(px(20.))
                                                .py(px(6.))
                                                .rounded(px(4.))
                                                .cursor_pointer()
                                                .when(can_save, |s: Stateful<Div>| {
                                                    s.bg(success())
                                                        .text_color(btn_fg())
                                                        .hover(|s| s.bg(btn_hover()))
                                                })
                                                .when(!can_save, |s: Stateful<Div>| {
                                                    s.bg(bg_selected())
                                                        .text_color(fg_dim())
                                                })
                                                .on_mouse_down(
                                                    MouseButton::Left,
                                                    cx.listener(|view, _, _, cx| {
                                                        view.save_form();
                                                        cx.notify();
                                                    }),
                                                )
                                                .child(div().text_size(px(13.)).child("Save")),
                                        )
                                        .child(
                                            div()
                                                .id("cancel-btn")
                                                .px(px(20.))
                                                .py(px(6.))
                                                .rounded(px(4.))
                                                .bg(bg_selected())
                                                .text_color(fg())
                                                .cursor_pointer()
                                                .hover(|s| s.bg(bg_hover()))
                                                .on_mouse_down(
                                                    MouseButton::Left,
                                                    cx.listener(|view, _, _, cx| {
                                                        view.mode = Mode::List;
                                                        view.clear_form();
                                                        cx.notify();
                                                    }),
                                                )
                                                .child(div().text_size(px(13.)).child("Cancel")),
                                        ),
                                ),
                        )
                    })
                    .when(mode == Mode::List, |this: Div| {
                        this.children(projects.into_iter().map(|(idx, proj)| {
                            let ws_count = proj.workspaces.len();
                            let active_count = proj.workspaces.iter().filter(|w| w.active).count();

                            div()
                                .id(ElementId::Name(format!("proj-{idx}").into()))
                                .px(px(20.))
                                .py(px(12.))
                                .border_b_1()
                                .border_color(border_color())
                                .hover(|s| s.bg(bg_hover()))
                                .flex()
                                .justify_between()
                                .items_center()
                                .child(
                                    div()
                                        .flex()
                                        .flex_col()
                                        .gap(px(4.))
                                        .child(
                                            div()
                                                .text_size(px(15.))
                                                .text_color(accent())
                                                .child(proj.name.clone()),
                                        )
                                        .child(
                                            div()
                                                .text_size(px(12.))
                                                .text_color(fg_dim())
                                                .child(format!("{}  ·  branch: {}", proj.repo, proj.branch)),
                                        )
                                        .child(
                                            div()
                                                .text_size(px(11.))
                                                .text_color(fg_dim())
                                                .child(format!(
                                                    "{ws_count} workspace{}  ·  {active_count} active",
                                                    if ws_count == 1 { "" } else { "s" }
                                                )),
                                        ),
                                )
                                .child(
                                    div()
                                        .flex()
                                        .gap(px(6.))
                                        .child(
                                            div()
                                                .id(ElementId::Name(format!("edit-{idx}").into()))
                                                .px(px(12.))
                                                .py(px(4.))
                                                .rounded(px(3.))
                                                .bg(bg_selected())
                                                .text_color(fg())
                                                .cursor_pointer()
                                                .hover(|s| s.bg(btn_bg()).text_color(btn_fg()))
                                                .on_mouse_down(
                                                    MouseButton::Left,
                                                    cx.listener(move |view, _, _, cx| {
                                                        view.start_edit(idx);
                                                        cx.notify();
                                                    }),
                                                )
                                                .child(div().text_size(px(12.)).child("Edit")),
                                        )
                                        .child(
                                            div()
                                                .id(ElementId::Name(format!("del-{idx}").into()))
                                                .px(px(12.))
                                                .py(px(4.))
                                                .rounded(px(3.))
                                                .bg(bg_selected())
                                                .text_color(danger())
                                                .cursor_pointer()
                                                .hover(|s| s.bg(danger()).text_color(btn_fg()))
                                                .on_mouse_down(
                                                    MouseButton::Left,
                                                    cx.listener(move |view, _, _, cx| {
                                                        view.delete_project(idx);
                                                        cx.notify();
                                                    }),
                                                )
                                                .child(div().text_size(px(12.)).child("Delete")),
                                        ),
                                )
                        }))
                        .when(self.config.projects.is_empty(), |this: Div| {
                            this.child(
                                div()
                                    .px(px(20.))
                                    .py(px(40.))
                                    .flex()
                                    .justify_center()
                                    .child(
                                        div()
                                            .text_size(px(14.))
                                            .text_color(fg_dim())
                                            .child("No projects configured. Click \"+ Add Project\" to get started."),
                                    ),
                            )
                        })
                    }),
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
                            .child("Esc to close  ·  Tab to switch fields  ·  Enter to save"),
                    ),
            )
            .on_key_down(cx.listener(|view, event: &KeyDownEvent, _window, cx| {
                if view.mode == Mode::List {
                    return;
                }
                let key = &event.keystroke.key;
                if key.len() == 1 && !event.keystroke.modifiers.control {
                    let ch = key.chars().next().unwrap();
                    if ch.is_alphanumeric() || ch == '/' || ch == '-' || ch == '_' || ch == '.' || ch == '~' {
                        view.push_char(ch);
                        cx.notify();
                    }
                } else if key == "backspace" {
                    view.pop_char();
                    cx.notify();
                }
            }))
    }
}

impl Focusable for ProjectsView {
    fn focus_handle(&self, _: &App) -> FocusHandle {
        self.focus.clone()
    }
}
