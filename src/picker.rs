use crate::text_input::{TextInput, TextInputEvent};
use crate::theme;
use crate::ui_helpers;
use gpui::prelude::FluentBuilder;
use gpui::*;
use std::sync::mpsc;

#[derive(Clone)]
pub struct PickerItem {
    pub name: String,
    pub project: String,
    pub active: bool,
    pub acp_status: Option<String>,
}

pub const CREATE_SENTINEL: &str = "\0CREATE";
pub const DESTROY_PREFIX: &str = "\0DESTROY\0";

pub enum PickerMode {
    List { items: Vec<PickerItem> },
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
    crate::text_input::bind_text_input_keys(cx);

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
            cx.new(move |cx| {
                let view = PickerView::new(mode, tx, window, cx);
                view.focus_initial_on_open(window, cx);
                view
            })
        },
    )
    .ok();
}

pub fn run_picker(mode: PickerMode) -> Option<String> {
    let (tx, rx) = mpsc::channel::<String>();

    let app = Application::new();
    app.run(move |cx: &mut App| {
        crate::text_input::bind_text_input_keys(cx);
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

actions!(ws_picker, [Cancel, Confirm, SelectNext, SelectPrev, TabForward, TabBack, OpenCreate, DestroySelected]);

#[derive(Clone, Copy, PartialEq)]
enum FormField {
    Name,
    Project,
    Repo,
    Branch,
}

struct PickerView {
    items: Vec<PickerItem>,
    filtered: Vec<usize>,
    query: Entity<TextInput>,
    selected: usize,
    freeform: bool,
    is_form: bool,
    form_field: FormField,
    form_name: Entity<TextInput>,
    form_project: Entity<TextInput>,
    form_repo: Entity<TextInput>,
    form_branch: Entity<TextInput>,
    projects: Vec<String>,
    project_filtered: Vec<usize>,
    project_selected: usize,
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
        let query = cx.new(|cx| TextInput::new("type to filter...", cx));
        let form_name = cx.new(|cx| TextInput::new("e.g. aleks/my-feature", cx));
        let form_project = cx.new(|cx| TextInput::new("select or type new name", cx));
        let form_repo = cx.new(|cx| TextInput::new("/path/to/git/repo", cx));
        let form_branch = cx.new(|cx| TextInput::new("master", cx));

        cx.subscribe_in(&query, _window, |view, _input, _event: &TextInputEvent, _window, cx| {
            view.filter(cx);
            cx.notify();
        })
        .detach();

        cx.subscribe_in(&form_project, _window, |view, _input, _event: &TextInputEvent, _window, cx| {
            view.filter_projects(cx);
            cx.notify();
        })
        .detach();

        match mode {
            PickerMode::List { items } => {
                let filtered = (0..items.len()).collect();
                Self {
                    items,
                    filtered,
                    query,
                    selected: 0,
                    freeform: false,
                    is_form: false,
                    form_field: FormField::Name,
                    form_name,
                    form_project,
                    form_repo,
                    form_branch,
                    projects: vec![],
                    project_filtered: vec![],
                    project_selected: 0,
                    tx,
                    focus,
                }
            }
            PickerMode::Freeform { prompt: _ } => Self {
                items: vec![],
                filtered: vec![],
                query,
                selected: 0,
                freeform: true,
                is_form: false,
                form_field: FormField::Name,
                form_name,
                form_project,
                form_repo,
                form_branch,
                projects: vec![],
                project_filtered: vec![],
                project_selected: 0,
                tx,
                focus,
            },
            PickerMode::CreateForm { projects } => {
                let project_filtered = (0..projects.len()).collect();
                Self {
                    items: vec![],
                    filtered: vec![],
                    query,
                    selected: 0,
                    freeform: false,
                    is_form: true,
                    form_field: FormField::Name,
                    form_name,
                    form_project,
                    form_repo,
                    form_branch,
                    projects,
                    project_filtered,
                    project_selected: 0,
                    tx,
                    focus,
                }
            }
        }
    }

    fn read_field(&self, field: FormField, cx: &App) -> String {
        match field {
            FormField::Name => self.form_name.read(cx).value().to_string(),
            FormField::Project => self.form_project.read(cx).value().to_string(),
            FormField::Repo => self.form_repo.read(cx).value().to_string(),
            FormField::Branch => self.form_branch.read(cx).value().to_string(),
        }
    }

    fn active_field_entity(&self) -> &Entity<TextInput> {
        match self.form_field {
            FormField::Name => &self.form_name,
            FormField::Project => &self.form_project,
            FormField::Repo => &self.form_repo,
            FormField::Branch => &self.form_branch,
        }
    }

    fn is_new_project(&self, cx: &App) -> bool {
        let project = self.form_project.read(cx).value().to_string();
        !project.is_empty() && !self.projects.iter().any(|p| p == &project)
    }

    fn filter(&mut self, cx: &App) {
        let q = self.query.read(cx).value().to_lowercase();
        if q.is_empty() {
            self.filtered = (0..self.items.len()).collect();
        } else {
            self.filtered = self
                .items
                .iter()
                .enumerate()
                .filter(|(_, item)| fuzzy_match(&item.name.to_lowercase(), &q))
                .map(|(i, _)| i)
                .collect();
        }
        self.selected = 0;
    }

    fn filter_projects(&mut self, cx: &App) {
        let q = self.form_project.read(cx).value().to_lowercase();
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

    fn next_form_field(&self, cx: &App) -> FormField {
        match self.form_field {
            FormField::Name => FormField::Project,
            FormField::Project => {
                if self.is_new_project(cx) {
                    FormField::Repo
                } else {
                    FormField::Name
                }
            }
            FormField::Repo => FormField::Branch,
            FormField::Branch => FormField::Name,
        }
    }

    fn prev_form_field(&self, cx: &App) -> FormField {
        match self.form_field {
            FormField::Name => {
                if self.is_new_project(cx) {
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

    fn focus_active_field(&self, window: &mut Window, cx: &App) {
        window.focus(&self.active_field_entity().read(cx).focus_handle(cx));
    }

    fn focus_initial_on_open(&self, window: &mut Window, cx: &Context<Self>) {
        if self.is_form {
            window.focus(&self.active_field_entity().read(cx).focus_handle(cx));
        } else {
            window.focus(&self.query.read(cx).focus_handle(cx));
        }
    }

    fn on_cancel(&mut self, _: &Cancel, window: &mut Window, _cx: &mut Context<Self>) {
        window.remove_window();
    }

    fn on_confirm(&mut self, _: &Confirm, window: &mut Window, cx: &mut Context<Self>) {
        if self.is_form {
            let project_val = self.read_field(FormField::Project, cx);
            if self.form_field == FormField::Project && !self.project_filtered.is_empty() {
                let idx = self.project_filtered[self.project_selected];
                self.form_project.update(cx, |input, cx| {
                    input.set_value(&self.projects[idx], cx);
                });
                self.form_field = self.next_form_field(cx);
                self.focus_active_field(window, cx);
                cx.notify();
                return;
            }
            let name_val = self.read_field(FormField::Name, cx);
            if name_val.is_empty() {
                self.form_field = FormField::Name;
                self.focus_active_field(window, cx);
                cx.notify();
                return;
            }
            if project_val.is_empty() {
                self.form_field = FormField::Project;
                self.focus_active_field(window, cx);
                cx.notify();
                return;
            }
            let repo_val = self.read_field(FormField::Repo, cx);
            if self.is_new_project(cx) && repo_val.is_empty() {
                self.form_field = FormField::Repo;
                self.focus_active_field(window, cx);
                cx.notify();
                return;
            }
            self.submit_form(window, cx);
            return;
        }

        let query_val = self.query.read(cx).value().to_string();
        let value = if self.freeform {
            if query_val.is_empty() {
                window.remove_window();
                return;
            }
            query_val
        } else if let Some(&idx) = self.filtered.get(self.selected) {
            self.items[idx].name.clone()
        } else {
            window.remove_window();
            return;
        };
        let _ = self.tx.send(value);
        window.remove_window();
    }

    fn submit_form(&mut self, window: &mut Window, cx: &App) {
        let is_new = self.is_new_project(cx);
        let name = self.read_field(FormField::Name, cx);
        let project = self.read_field(FormField::Project, cx);
        let repo = self.read_field(FormField::Repo, cx);
        let branch_val = self.read_field(FormField::Branch, cx);
        let branch = if branch_val.is_empty() {
            "master".to_string()
        } else {
            branch_val
        };
        let result = format!(
            "{}\0{}\0{}\0{}\0{}",
            name,
            project,
            repo,
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

    fn on_tab(&mut self, _: &TabForward, window: &mut Window, cx: &mut Context<Self>) {
        if self.is_form {
            self.form_field = self.next_form_field(cx);
            self.focus_active_field(window, cx);
            cx.notify();
        } else if !self.filtered.is_empty() {
            self.selected = (self.selected + 1) % self.filtered.len();
            cx.notify();
        }
    }

    fn on_tab_back(&mut self, _: &TabBack, window: &mut Window, cx: &mut Context<Self>) {
        if self.is_form {
            self.form_field = self.prev_form_field(cx);
            self.focus_active_field(window, cx);
            cx.notify();
        }
    }

    fn on_open_create(&mut self, _: &OpenCreate, window: &mut Window, _cx: &mut Context<Self>) {
        let _ = self.tx.send(CREATE_SENTINEL.to_string());
        window.remove_window();
    }

    fn on_destroy_selected(&mut self, _: &DestroySelected, window: &mut Window, _cx: &mut Context<Self>) {
        if self.is_form || self.freeform {
            return;
        }
        if let Some(&idx) = self.filtered.get(self.selected) {
            let name = self.items[idx].name.clone();
            let _ = self.tx.send(format!("{DESTROY_PREFIX}{name}"));
            window.remove_window();
        }
    }
}

fn bg() -> Rgba { theme::bg() }
fn bg_hover() -> Rgba { theme::bg_hover() }
fn bg_selected() -> Rgba { theme::bg_selected() }
fn fg() -> Rgba { theme::fg() }
fn fg_dim() -> Rgba { theme::fg_dim() }
fn accent() -> Rgba { theme::accent() }
fn active_dot() -> Rgba { theme::active_dot() }
fn success() -> Rgba { theme::success() }
fn border_color() -> Rgba { theme::border_color() }
fn btn_bg() -> Rgba { theme::btn_bg() }
fn btn_fg() -> Rgba { theme::btn_fg() }
fn btn_hover() -> Rgba { theme::btn_hover() }
fn new_badge() -> Rgba { theme::new_badge() }
fn new_badge_fg() -> Rgba { theme::new_badge_fg() }

fn render_form_field(
    label: &str,
    input: &Entity<TextInput>,
    focused: bool,
    field: FormField,
    cx: &mut Context<'_, PickerView>,
) -> Stateful<Div> {
    let input_entity = input.clone();
    ui_helpers::render_form_field(label, input, focused, move |view: &mut PickerView, window, cx| {
        view.form_field = field;
        window.focus(&input_entity.read(cx).focus_handle(cx));
    }, cx)
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
                    cx.listener(move |view, _, window, cx| {
                        match field {
                            FormField::Project => {
                                view.form_project.update(cx, |input, cx| {
                                    input.set_value(&item_for_click, cx);
                                });
                                view.form_field = FormField::Name;
                                view.focus_active_field(window, cx);
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
        let name_val = self.read_field(FormField::Name, cx);
        let project_val = self.read_field(FormField::Project, cx);
        let repo_val = self.read_field(FormField::Repo, cx);
        let is_new = self.is_new_project(cx);
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
                        &self.form_name,
                        field == FormField::Name,
                        FormField::Name,
                        cx,
                    ))
                    .child(
                        div()
                            .flex()
                            .flex_col()
                            .gap(px(4.))
                            .child(render_form_field(
                                "PROJECT",
                                &self.form_project,
                                field == FormField::Project,
                                FormField::Project,
                                cx,
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
                            &self.form_repo,
                            field == FormField::Repo,
                            FormField::Repo,
                            cx,
                        ))
                        .child(render_form_field(
                            "SOURCE BRANCH",
                            &self.form_branch,
                            field == FormField::Branch,
                            FormField::Branch,
                            cx,
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
                                    let is_new = view.is_new_project(cx);
                                    let name = view.read_field(FormField::Name, cx);
                                    let project = view.read_field(FormField::Project, cx);
                                    let repo = view.read_field(FormField::Repo, cx);
                                    let can_create = !name.is_empty()
                                        && !project.is_empty()
                                        && (!is_new || !repo.is_empty());
                                    if !can_create {
                                        return;
                                    }
                                    view.submit_form(window, cx);
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
    }

    fn render_picker(&mut self, cx: &mut Context<Self>) -> Div {
        let filtered = self.filtered.clone();
        let selected = self.selected;
        let items = &self.items;

        let mut groups: Vec<(String, Vec<(usize, usize)>)> = Vec::new();
        for (vi, &item_idx) in filtered.iter().enumerate() {
            let project = &items[item_idx].project;
            if let Some(last) = groups.last_mut() {
                if &last.0 == project {
                    last.1.push((vi, item_idx));
                    continue;
                }
            }
            groups.push((project.clone(), vec![(vi, item_idx)]));
        }

        div()
            .key_context("Picker")
            .track_focus(&self.focus)
            .on_action(cx.listener(Self::on_cancel))
            .on_action(cx.listener(Self::on_confirm))
            .on_action(cx.listener(Self::on_next))
            .on_action(cx.listener(Self::on_prev))
            .on_action(cx.listener(Self::on_tab))
            .on_action(cx.listener(Self::on_tab_back))
            .on_action(cx.listener(Self::on_open_create))
            .on_action(cx.listener(Self::on_destroy_selected))
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
                    .flex()
                    .items_center()
                    .justify_between()
                    .child(
                        div()
                            .flex()
                            .items_center()
                            .gap(px(8.))
                            .child(
                                div()
                                    .text_color(accent())
                                    .text_size(px(16.))
                                    .child("Select Workspace"),
                            )
                            .child(
                                div()
                                    .text_size(px(14.))
                                    .text_color(fg())
                                    .flex_1()
                                    .child(self.query.clone()),
                            ),
                    )
                    .child(
                        div()
                            .id("create-btn")
                            .px(px(8.))
                            .py(px(2.))
                            .rounded(px(4.))
                            .bg(btn_bg())
                            .text_color(btn_fg())
                            .text_size(px(16.))
                            .cursor_pointer()
                            .hover(|s| s.bg(btn_hover()))
                            .on_mouse_down(
                                MouseButton::Left,
                                cx.listener(|view, _, window, _cx| {
                                    let _ = view.tx.send(CREATE_SENTINEL.to_string());
                                    window.remove_window();
                                }),
                            )
                            .child("+"),
                    ),
            )
            .when(!self.freeform, |this: Div| {
                let mut list = div().flex().flex_col().flex_1().overflow_y_hidden();
                for (project, entries) in groups {
                    list = list.child(
                        div()
                            .px(px(16.))
                            .pt(px(10.))
                            .pb(px(4.))
                            .child(
                                div()
                                    .text_size(px(11.))
                                    .text_color(fg_dim())
                                    .child(project.to_uppercase()),
                            ),
                    );
                    for (vi, item_idx) in entries {
                        let item = items[item_idx].clone();
                        let is_selected = vi == selected;

                        list = list.child(
                            div()
                                .id(ElementId::Name(format!("item-{vi}").into()))
                                .px(px(16.))
                                .py(px(4.))
                                .bg(if is_selected { bg_selected() } else { bg() })
                                .hover(|s| s.bg(bg_hover()))
                                .cursor_pointer()
                                .on_mouse_down(
                                    MouseButton::Left,
                                    cx.listener(move |view, _, window, _cx| {
                                        let _ = view.tx.send(view.items[item_idx].name.clone());
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
                                                .when(item.active, |s: Div| s.bg(active_dot()))
                                                .when(!item.active, |s: Div| {
                                                    s.bg(gpui::transparent_black())
                                                }),
                                        )
                                        .child(
                                            div()
                                                .flex()
                                                .flex_1()
                                                .items_center()
                                                .gap(px(6.))
                                                .child(
                                                    div()
                                                        .text_size(px(14.))
                                                        .when(is_selected, |s: Div| {
                                                            s.text_color(accent())
                                                        })
                                                        .child(item.name.clone()),
                                                )
                                                .when_some(item.acp_status.clone(), |s, status| {
                                                    let color = if status == "running" {
                                                        success()
                                                    } else {
                                                        fg_dim()
                                                    };
                                                    s.child(
                                                        div()
                                                            .text_size(px(10.))
                                                            .text_color(color)
                                                            .child("ACP"),
                                                    )
                                                }),
                                        ),
                                ),
                        );
                    }
                }
                this.child(list)
            })
            .child(
                div()
                    .px(px(12.))
                    .py(px(8.))
                    .border_t_1()
                    .border_color(border_color())
                    .child(
                        div()
                            .text_size(px(11.))
                            .text_color(fg_dim())
                            .child("↑↓ navigate  ·  Enter open  ·  Ctrl+N create  ·  Ctrl+D destroy  ·  Esc close"),
                    ),
            )
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

#[cfg(test)]
mod tests {
    use super::{fuzzy_match, parse_create_result, CREATE_SENTINEL, DESTROY_PREFIX};

    #[test]
    fn fuzzy_match_exact() {
        assert!(fuzzy_match("hello", "hello"));
    }

    #[test]
    fn fuzzy_match_subsequence() {
        assert!(fuzzy_match("hello world", "hlo"));
    }

    #[test]
    fn fuzzy_match_empty_pattern() {
        assert!(fuzzy_match("anything", ""));
    }

    #[test]
    fn fuzzy_match_no_match() {
        assert!(!fuzzy_match("abc", "xyz"));
    }

    #[test]
    fn fuzzy_match_pattern_longer() {
        assert!(!fuzzy_match("ab", "abc"));
    }

    #[test]
    fn fuzzy_match_order_matters() {
        assert!(fuzzy_match("abcdef", "ace"));
        assert!(!fuzzy_match("abcdef", "eca"));
    }

    #[test]
    fn fuzzy_match_repeated_chars() {
        assert!(fuzzy_match("aabbc", "abc"));
    }

    #[test]
    fn parse_create_result_valid() {
        let result = parse_create_result("name\0proj\0/repo\0main\01").unwrap();
        assert_eq!(result.name, "name");
        assert_eq!(result.project, "proj");
        assert_eq!(result.repo_path, "/repo");
        assert_eq!(result.branch, "main");
        assert!(result.is_new_project);
    }

    #[test]
    fn parse_create_result_existing_project() {
        let result = parse_create_result("ws\0proj\0/r\0master\00").unwrap();
        assert!(!result.is_new_project);
    }

    #[test]
    fn parse_create_result_too_few_parts() {
        assert!(parse_create_result("a\0b\0c").is_none());
    }

    #[test]
    fn parse_create_result_empty_fields() {
        let result = parse_create_result("\0\0\0\0").unwrap();
        assert!(result.name.is_empty());
        assert!(result.project.is_empty());
    }

    #[test]
    fn create_sentinel_is_nul_prefixed() {
        assert!(CREATE_SENTINEL.starts_with('\0'));
    }

    #[test]
    fn destroy_prefix_is_nul_prefixed() {
        assert!(DESTROY_PREFIX.starts_with('\0'));
    }
}
