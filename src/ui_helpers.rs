use crate::text_input::TextInput;
use crate::theme;
use gpui::prelude::FluentBuilder;
use gpui::*;

/// Standard centered window options shared by every awesometree GPUI window.
pub fn centered_window_options(
    cx: &mut App,
    app_id: &str,
    width: f32,
    height: f32,
) -> WindowOptions {
    let bounds = Bounds::centered(None, size(px(width), px(height)), cx);
    WindowOptions {
        window_bounds: Some(WindowBounds::Windowed(bounds)),
        titlebar: None,
        app_id: Some(app_id.into()),
        window_decorations: Some(WindowDecorations::Server),
        ..Default::default()
    }
}

/// Visual style of an action [`button`].
#[derive(Clone, Copy, PartialEq)]
pub enum ButtonKind {
    /// Solid accent button for the primary action.
    Primary,
    /// Solid green button for confirm/save actions.
    Success,
    /// Muted button for secondary actions.
    Secondary,
    /// Solid destructive action button.
    Danger,
    /// Greyed-out, non-interactive appearance.
    Disabled,
}

/// A standard action button (medium size) used in headers and form footers.
pub fn button<V: 'static>(
    id: impl Into<ElementId>,
    label: impl Into<SharedString>,
    kind: ButtonKind,
    on_click: impl Fn(&mut V, &mut Window, &mut Context<V>) + 'static,
    cx: &mut Context<'_, V>,
) -> Stateful<Div> {
    let (bg_color, fg_color) = match kind {
        ButtonKind::Primary => (theme::btn_bg(), theme::btn_fg()),
        ButtonKind::Success => (theme::success(), theme::btn_fg()),
        ButtonKind::Secondary => (theme::bg_selected(), theme::fg()),
        ButtonKind::Danger => (theme::danger(), theme::btn_fg()),
        ButtonKind::Disabled => (theme::bg_selected(), theme::fg_dim()),
    };
    div()
        .id(id.into())
        .px(px(16.))
        .py(px(6.))
        .rounded(px(4.))
        .bg(bg_color)
        .text_color(fg_color)
        .text_size(px(13.))
        .when(kind != ButtonKind::Disabled, |s| {
            s.cursor_pointer()
                .map(|s| match kind {
                    ButtonKind::Primary | ButtonKind::Success => {
                        s.hover(|s| s.bg(theme::btn_hover()))
                    }
                    ButtonKind::Secondary => s.hover(|s| s.bg(theme::bg_hover())),
                    ButtonKind::Danger => s.hover(|s| s.opacity(0.85)),
                    ButtonKind::Disabled => s,
                })
                .on_mouse_down(
                    MouseButton::Left,
                    cx.listener(move |view, _, window, cx| {
                        on_click(view, window, cx);
                        cx.notify();
                    }),
                )
        })
        .child(label.into())
}

/// Visual style of a small inline [`chip_button`].
#[derive(Clone, Copy, PartialEq)]
pub enum ChipKind {
    /// Muted chip that fills with the accent colour on hover.
    Neutral,
    /// Muted chip with destructive text that fills red on hover.
    Danger,
}

/// A small inline "chip" button used inside list rows (Edit / Delete / Stop …).
pub fn chip_button<V: 'static>(
    id: impl Into<ElementId>,
    label: impl Into<SharedString>,
    kind: ChipKind,
    on_click: impl Fn(&mut V, &mut Window, &mut Context<V>) + 'static,
    cx: &mut Context<'_, V>,
) -> Stateful<Div> {
    let fg_color = match kind {
        ChipKind::Neutral => theme::fg(),
        ChipKind::Danger => theme::danger(),
    };
    div()
        .id(id.into())
        .px(px(12.))
        .py(px(4.))
        .rounded(px(3.))
        .bg(theme::bg_selected())
        .text_color(fg_color)
        .text_size(px(12.))
        .cursor_pointer()
        .map(|s| match kind {
            ChipKind::Neutral => s.hover(|s| s.bg(theme::btn_bg()).text_color(theme::btn_fg())),
            ChipKind::Danger => s.hover(|s| s.bg(theme::danger()).text_color(theme::btn_fg())),
        })
        .on_mouse_down(
            MouseButton::Left,
            cx.listener(move |view, _, window, cx| {
                on_click(view, window, cx);
                cx.notify();
            }),
        )
        .child(label.into())
}

/// A 16px checkbox/indicator that renders a check mark when `checked`.
pub fn checkbox(checked: bool) -> Div {
    div()
        .size(px(16.))
        .rounded(px(3.))
        .border_1()
        .border_color(if checked { theme::accent() } else { theme::fg_dim() })
        .when(checked, |s: Div| s.bg(theme::accent()))
        .flex()
        .items_center()
        .justify_center()
        .when(checked, |s: Div| {
            s.child(
                div()
                    .text_size(px(11.))
                    .text_color(theme::btn_fg())
                    .child("✓"),
            )
        })
}

/// Shared visual style for the bordered text-input box used across forms.
fn field_box_style<E: Styled>(el: E, focused: bool) -> E {
    el.px(px(10.))
        .py(px(6.))
        .rounded(px(4.))
        .border_1()
        .border_color(if focused { theme::border_focus() } else { theme::border_color() })
        .bg(theme::bg_hover())
        .text_size(px(14.))
        .text_color(theme::fg())
        .font_family("monospace")
}

/// A standalone, clickable bordered input box (for rows that aren't full form fields).
pub fn field_box<V: 'static>(
    id: impl Into<ElementId>,
    input: &Entity<TextInput>,
    focused: bool,
    on_click: impl Fn(&mut V, &mut Window, &mut Context<V>) + 'static,
    cx: &mut Context<'_, V>,
) -> Stateful<Div> {
    let el = div()
        .id(id.into())
        .flex_1()
        .cursor(CursorStyle::IBeam)
        .on_mouse_down(
            MouseButton::Left,
            cx.listener(move |view, _, window, cx| {
                on_click(view, window, cx);
                cx.notify();
            }),
        );
    field_box_style(el, focused).child(input.clone())
}

/// A labelled form field: an uppercase label above a bordered input box.
pub fn render_form_field<V: 'static>(
    label: &str,
    input: &Entity<TextInput>,
    focused: bool,
    on_click: impl Fn(&mut V, &mut Window, &mut Context<V>) + 'static,
    cx: &mut Context<'_, V>,
) -> Stateful<Div> {
    div()
        .id(ElementId::Name(format!("field-{label}").into()))
        .cursor(CursorStyle::IBeam)
        .on_mouse_down(
            MouseButton::Left,
            cx.listener(move |view, _, window, cx| {
                on_click(view, window, cx);
                cx.notify();
            }),
        )
        .flex()
        .flex_col()
        .gap(px(4.))
        .child(
            div()
                .text_size(px(12.))
                .text_color(if focused { theme::accent() } else { theme::fg_dim() })
                .child(label.to_string()),
        )
        .child(field_box_style(div(), focused).child(input.clone()))
}
