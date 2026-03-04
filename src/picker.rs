use gpui::prelude::FluentBuilder;
use gpui::*;
use std::sync::mpsc;

pub enum PickerMode {
    List { items: Vec<String>, prompt: String },
    Freeform { prompt: String },
    CreateForm {
        projects: Vec<String>,
    },
}

pub struct CreateFormResult {
    pub name: String,
    pub project: String,
    pub repo_path: String,
    pub branch: String,
    pub is_new_project: bool,
}

pub fn open_picker_window(cx: &mut App, mode: PickerMode, tx: mpsc::Sender<String>) {
    let bounds = Bounds::centered(None, size(px(500.), px(420.)), cx);
    cx.open_window(
        WindowOptions {
            window_bounds: Some(WindowBounds::Windowed(bounds)),
            titlebar: None,
            app_id: Some("awesometree-picker".into()),
            window_decorations: Some(WindowDecorations::Server),
            ..Default::default()
        },
        move |window, cx| {
            let tx = tx.clone();
            cx.new(move |cx| PickerView::new(mode, tx, window, cx))
        },
    )
    .ok();
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
            KeyBinding::new("tab", TabForward, None),
            KeyBinding::new("shift-tab", TabBack, None),
        ]);

        open_picker_window(cx, mode, tx);
    });

    rx.try_recv().ok()
}

pub fn parse_create_result(s: &str) -> Option<CreateFormResult> {
    let parts: Vec<&str> = s.splitn(5, '\0').collect();
    if parts.len() == 5 {
        Some(CreateFormResult {
            name: parts[0].to_string(),
            project: parts[1].to_string(),
            repo_path: parts[2].to_string(),
            branch: parts[3].to_string(),
            is_new_project: parts[4] == "1",
        })
    } else {
        None
    }
}

actions!(ws_picker, [Cancel, Confirm, SelectNext, SelectPrev, TabForward, TabBack]);

#[derive(Clone, Copy, PartialEq)]
enum FormField {
    Name,
    Project,
    Repo,
    Branch,
}

struct PickerView {
    items: Vec<String>,
    filtered: Vec<usize>,
    query: String,
    selected: usize,
    freeform: bool,
    is_form: bool,
    form_field: FormField,
    form_name: String,
    form_project: String,
    form_repo: String,
    form_branch: String,
    projects: Vec<String>,
    project_filtered: Vec<usize>,
    project_selected: usize,
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
        let focus = cx.focus_handle();
        match mode {
            PickerMode::List { items, prompt } => {
                let filtered = (0..items.len()).collect();
                Self {
                    items,
                    filtered,
                    query: String::new(),
                    selected: 0,
                    freeform: false,
                    is_form: false,
                    form_field: FormField::Name,
                    form_name: String::new(),
                    form_project: String::new(),
                    form_repo: String::new(),
                    form_branch: String::new(),
                    projects: vec![],
                    project_filtered: vec![],
                    project_selected: 0,
                    prompt,
                    tx,
                    focus,
                }
            }
            PickerMode::Freeform { prompt } => Self {
                items: vec![],
                filtered: vec![],
                query: String::new(),
                selected: 0,
                freeform: true,
                is_form: false,
                form_field: FormField::Name,
                form_name: String::new(),
                form_project: String::new(),
                form_repo: String::new(),
                form_branch: String::new(),
                projects: vec![],
                project_filtered: vec![],
                project_selected: 0,
                prompt,
                tx,
                focus,
            },
            PickerMode::CreateForm { projects } => {
                let project_filtered = (0..projects.len()).collect();
                Self {
                    items: vec![],
                    filtered: vec![],
                    query: String::new(),
                    selected: 0,
                    freeform: false,
                    is_form: true,
                    form_field: FormField::Name,
                    form_name: String::new(),
                    form_project: String::new(),
                    form_repo: String::new(),
                    form_branch: String::new(),
                    projects,
                    project_filtered,
                    project_selected: 0,
                    prompt: String::new(),
                    tx,
                    focus,
                }
            }
        }
    }

    fn is_new_project(&self) -> bool {
        !self.form_project.is_empty()
            && !self.projects.iter().any(|p| p == &self.form_project)
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

    fn filter_projects(&mut self) {
        let q = self.form_project.to_lowercase();
        if q.is_empty() {
            self.project_filtered = (0..self.projects.len()).collect();
        } else {
            self.project_filtered = self
                .projects
                .iter()
                .enumerate()
                .filter(|(_, item)| fuzzy_match(&item.to_lowercase(), &q))
                .map(|(i, _)| i)
                .collect();
        }
        self.project_selected = 0;
    }

    fn push_char_to_field(&mut self, ch: char) {
        match self.form_field {
            FormField::Name => self.form_name.push(ch),
            FormField::Project => {
                self.form_project.push(ch);
                self.filter_projects();
            }
            FormField::Repo => self.form_repo.push(ch),
            FormField::Branch => self.form_branch.push(ch),
        }
    }

    fn pop_char_from_field(&mut self) {
        match self.form_field {
            FormField::Name => { self.form_name.pop(); }
            FormField::Project => {
                self.form_project.pop();
                self.filter_projects();
            }
            FormField::Repo => { self.form_repo.pop(); }
            FormField::Branch => { self.form_branch.pop(); }
        }
    }

    fn next_form_field(&self) -> FormField {
        match self.form_field {
            FormField::Name => FormField::Project,
            FormField::Project => {
                if self.is_new_project() {
                    FormField::Repo
                } else {
                    FormField::Name
                }
            }
            FormField::Repo => FormField::Branch,
            FormField::Branch => FormField::Name,
        }
    }

    fn prev_form_field(&self) -> FormField {
        match self.form_field {
            FormField::Name => {
                if self.is_new_project() {
                    FormField::Branch
                } else {
                    FormField::Project
                }
            }
            FormField::Project => FormField::Name,
            FormField::Repo => FormField::Project,
            FormField::Branch => FormField::Repo,
        }
    }

    fn on_cancel(&mut self, _: &Cancel, window: &mut Window, _cx: &mut Context<Self>) {
        window.remove_window();
    }

    fn on_confirm(&mut self, _: &Confirm, window: &mut Window, cx: &mut Context<Self>) {
        if self.is_form {
            if self.form_field == FormField::Project && !self.project_filtered.is_empty() {
                let idx = self.project_filtered[self.project_selected];
                self.form_project = self.projects[idx].clone();
                self.form_field = self.next_form_field();
                cx.notify();
                return;
            }
            if self.form_name.is_empty() {
                self.form_field = FormField::Name;
                cx.notify();
                return;
            }
            if self.form_project.is_empty() {
                self.form_field = FormField::Project;
                cx.notify();
                return;
            }
            if self.is_new_project() && self.form_repo.is_empty() {
                self.form_field = FormField::Repo;
                cx.notify();
                return;
            }
            self.submit_form(window);
            return;
        }

        let value = if self.freeform {
            if self.query.is_empty() {
                window.remove_window();
                return;
            }
            self.query.clone()
        } else if let Some(&idx) = self.filtered.get(self.selected) {
            self.items[idx].clone()
        } else {
            window.remove_window();
            return;
        };
        let _ = self.tx.send(value);
        window.remove_window();
    }

    fn submit_form(&mut self, window: &mut Window) {
        let is_new = self.is_new_project();
        let branch = if self.form_branch.is_empty() {
            "master".to_string()
        } else {
            self.form_branch.clone()
        };
        let result = format!(
            "{}\0{}\0{}\0{}\0{}",
            self.form_name,
            self.form_project,
            self.form_repo,
            branch,
            if is_new { "1" } else { "0" },
        );
        let _ = self.tx.send(result);
        window.remove_window();
    }

    fn on_next(&mut self, _: &SelectNext, _window: &mut Window, cx: &mut Context<Self>) {
        if self.is_form {
            if self.form_field == FormField::Project && !self.project_filtered.is_empty() {
                self.project_selected = (self.project_selected + 1) % self.project_filtered.len();
            }
            cx.notify();
            return;
        }
        if !self.filtered.is_empty() {
            self.selected = (self.selected + 1) % self.filtered.len();
            cx.notify();
        }
    }

    fn on_prev(&mut self, _: &SelectPrev, _window: &mut Window, cx: &mut Context<Self>) {
        if self.is_form {
            if self.form_field == FormField::Project && !self.project_filtered.is_empty() {
                self.project_selected = self.project_selected.checked_sub(1).unwrap_or(self.project_filtered.len() - 1);
            }
            cx.notify();
            return;
        }
        if !self.filtered.is_empty() {
            self.selected = self
                .selected
                .checked_sub(1)
                .unwrap_or(self.filtered.len() - 1);
            cx.notify();
        }
    }

    fn on_tab(&mut self, _: &TabForward, _window: &mut Window, cx: &mut Context<Self>) {
        if self.is_form {
            self.form_field = self.next_form_field();
            cx.notify();
        } else if !self.filtered.is_empty() {
            self.selected = (self.selected + 1) % self.filtered.len();
            cx.notify();
        }
    }

    fn on_tab_back(&mut self, _: &TabBack, _window: &mut Window, cx: &mut Context<Self>) {
        if self.is_form {
            self.form_field = self.prev_form_field();
            cx.notify();
        }
    }
}

fn bg() -> Rgba { rgba(0x1e1e2eff) }
fn bg_hover() -> Rgba { rgba(0x313244ff) }
fn bg_selected() -> Rgba { rgba(0x45475aff) }
fn fg() -> Rgba { rgba(0xcdd6f4ff) }
fn fg_dim() -> Rgba { rgba(0x6c7086ff) }
fn accent() -> Rgba { rgba(0x89b4faff) }
fn active_dot() -> Rgba { rgba(0xa6e3a1ff) }
fn border_color() -> Rgba { rgba(0x313244ff) }
fn border_focus() -> Rgba { rgba(0x89b4faff) }
fn btn_bg() -> Rgba { rgba(0x89b4faff) }
fn btn_fg() -> Rgba { rgba(0x1e1e2eff) }
fn btn_hover() -> Rgba { rgba(0xb4d0fbff) }
fn new_badge() -> Rgba { rgba(0xf9e2afff) }
fn new_badge_fg() -> Rgba { rgba(0x1e1e2eff) }

fn render_form_field(label: &str, value: &str, focused: bool, placeholder: &str) -> Div {
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

fn render_dropdown_items(
    items: &[String],
    filtered: &[usize],
    selected: usize,
    prefix: &str,
    cx: &mut Context<'_, PickerView>,
    field: FormField,
) -> Div {
    let max_visible = 5;
    let visible: Vec<_> = filtered.iter().take(max_visible).copied().collect();

    div().flex().flex_col().children(
        visible.into_iter().enumerate().map(|(vi, item_idx)| {
            let item = items[item_idx].clone();
            let is_selected = vi == selected;
            let item_for_click = item.clone();

            div()
                .id(ElementId::Name(format!("{prefix}-{vi}").into()))
                .px(px(10.))
                .py(px(3.))
                .rounded(px(2.))
                .bg(if is_selected { bg_selected() } else { bg() })
                .hover(|s| s.bg(bg_hover()))
                .cursor_pointer()
                .on_mouse_down(
                    MouseButton::Left,
                    cx.listener(move |view, _, _, cx| {
                        match field {
                            FormField::Project => {
                                view.form_project = item_for_click.clone();
                                view.form_field = FormField::Name;
                            }
                            _ => {}
                        }
                        cx.notify();
                    }),
                )
                .child(
                    div()
                        .text_size(px(13.))
                        .when(is_selected, |s: Div| s.text_color(accent()))
                        .child(item),
                )
        }),
    )
}

impl Render for PickerView {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        if self.is_form {
            return self.render_form(cx);
        }
        self.render_picker(cx)
    }
}

impl PickerView {
    fn render_form(&mut self, cx: &mut Context<Self>) -> Div {
        let project_filtered = self.project_filtered.clone();
        let project_selected = self.project_selected;
        let projects = self.projects.clone();
        let field = self.form_field;
        let name_val = self.form_name.clone();
        let project_val = self.form_project.clone();
        let repo_val = self.form_repo.clone();
        let branch_val = self.form_branch.clone();
        let is_new = self.is_new_project();
        let can_create = !name_val.is_empty()
            && !project_val.is_empty()
            && (!is_new || !repo_val.is_empty());

        div()
            .key_context("Picker")
            .track_focus(&self.focus)
            .on_action(cx.listener(Self::on_cancel))
            .on_action(cx.listener(Self::on_confirm))
            .on_action(cx.listener(Self::on_next))
            .on_action(cx.listener(Self::on_prev))
            .on_action(cx.listener(Self::on_tab))
            .on_action(cx.listener(Self::on_tab_back))
            .flex()
            .flex_col()
            .size_full()
            .bg(bg())
            .text_color(fg())
            .font_family("monospace")
            .child(
                div()
                    .px(px(16.))
                    .py(px(12.))
                    .border_b_1()
                    .border_color(border_color())
                    .flex()
                    .items_center()
                    .gap(px(10.))
                    .child(
                        div()
                            .text_size(px(16.))
                            .text_color(accent())
                            .child("Create Workspace"),
                    )
                    .when(is_new, |s: Div| {
                        s.child(
                            div()
                                .px(px(8.))
                                .py(px(2.))
                                .rounded(px(3.))
                                .bg(new_badge())
                                .text_color(new_badge_fg())
                                .text_size(px(10.))
                                .child("+ NEW PROJECT"),
                        )
                    }),
            )
            .child(
                div()
                    .flex()
                    .flex_col()
                    .flex_1()
                    .px(px(16.))
                    .py(px(12.))
                    .gap(px(12.))
                    .child(render_form_field(
                        "NAME",
                        &name_val,
                        field == FormField::Name,
                        "e.g. aleks/my-feature",
                    ))
                    .child(
                        div()
                            .flex()
                            .flex_col()
                            .gap(px(4.))
                            .child(render_form_field(
                                "PROJECT",
                                &project_val,
                                field == FormField::Project,
                                "select or type new name",
                            ))
                            .when(field == FormField::Project && !project_filtered.is_empty(), |this: Div| {
                                this.child(render_dropdown_items(
                                    &projects,
                                    &project_filtered,
                                    project_selected,
                                    "project",
                                    cx,
                                    FormField::Project,
                                ))
                            }),
                    )
                    .when(is_new, |this: Div| {
                        this.child(render_form_field(
                            "REPO PATH",
                            &repo_val,
                            field == FormField::Repo,
                            "/path/to/git/repo",
                        ))
                        .child(render_form_field(
                            "SOURCE BRANCH",
                            &branch_val,
                            field == FormField::Branch,
                            "master",
                        ))
                    }),
            )
            .child(
                div()
                    .px(px(16.))
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
                            .child("Tab to switch fields  ·  Enter to confirm  ·  Esc to cancel"),
                    )
                    .child(
                        div()
                            .id("create-btn")
                            .px(px(20.))
                            .py(px(6.))
                            .rounded(px(4.))
                            .cursor_pointer()
                            .when(can_create, |s: Stateful<Div>| {
                                s.bg(btn_bg())
                                    .text_color(btn_fg())
                                    .hover(|s| s.bg(btn_hover()))
                            })
                            .when(!can_create, |s: Stateful<Div>| {
                                s.bg(bg_selected())
                                    .text_color(fg_dim())
                            })
                            .on_mouse_down(
                                MouseButton::Left,
                                cx.listener(|view, _, window, cx| {
                                    let is_new = view.is_new_project();
                                    let can_create = !view.form_name.is_empty()
                                        && !view.form_project.is_empty()
                                        && (!is_new || !view.form_repo.is_empty());
                                    if !can_create {
                                        return;
                                    }
                                    view.submit_form(window);
                                    cx.notify();
                                }),
                            )
                            .child(
                                div()
                                    .text_size(px(13.))
                                    .child("Create"),
                            ),
                    ),
            )
            .on_key_down(cx.listener(|view, event: &KeyDownEvent, _window, cx| {
                let key = &event.keystroke.key;
                if key.len() == 1 && !event.keystroke.modifiers.control {
                    let ch = key.chars().next().unwrap();
                    if ch.is_alphanumeric() || ch == '/' || ch == '-' || ch == '_' || ch == '.' {
                        view.push_char_to_field(ch);
                        cx.notify();
                    }
                } else if key == "backspace" {
                    view.pop_char_from_field();
                    cx.notify();
                }
            }))
    }

    fn render_picker(&mut self, cx: &mut Context<Self>) -> Div {
        let filtered = self.filtered.clone();
        let selected = self.selected;

        div()
            .key_context("Picker")
            .track_focus(&self.focus)
            .on_action(cx.listener(Self::on_cancel))
            .on_action(cx.listener(Self::on_confirm))
            .on_action(cx.listener(Self::on_next))
            .on_action(cx.listener(Self::on_prev))
            .on_action(cx.listener(Self::on_tab))
            .on_action(cx.listener(Self::on_tab_back))
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
                                    cx.listener(move |view, _, window, _cx| {
                                        let _ = view.tx.send(view.items[item_idx].clone());
                                        window.remove_window();
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
