use crate::text_input::TextInput;
use crate::theme;
use gpui::*;

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
        .child(
            div()
                .px(px(10.))
                .py(px(6.))
                .rounded(px(4.))
                .border_1()
                .border_color(if focused { theme::border_focus() } else { theme::border_color() })
                .bg(theme::bg_hover())
                .text_size(px(14.))
                .text_color(theme::fg())
                .font_family("monospace")
                .child(input.clone()),
        )
}
