use gpui::rgba;
pub type Color = gpui::Rgba;

pub fn bg() -> Color { rgba(0x1e1e2eff) }
pub fn bg_hover() -> Color { rgba(0x313244ff) }
pub fn bg_selected() -> Color { rgba(0x45475aff) }
pub fn fg() -> Color { rgba(0xcdd6f4ff) }
pub fn fg_dim() -> Color { rgba(0x6c7086ff) }
pub fn accent() -> Color { rgba(0x89b4faff) }
pub fn active_dot() -> Color { rgba(0xa6e3a1ff) }
pub fn border_color() -> Color { rgba(0x313244ff) }
pub fn border_focus() -> Color { rgba(0x89b4faff) }
pub fn btn_bg() -> Color { rgba(0x89b4faff) }
pub fn btn_fg() -> Color { rgba(0x1e1e2eff) }
pub fn btn_hover() -> Color { rgba(0xb4d0fbff) }
pub fn danger() -> Color { rgba(0xf38ba8ff) }
pub fn success() -> Color { rgba(0xa6e3a1ff) }
pub fn new_badge() -> Color { rgba(0xf9e2afff) }
pub fn new_badge_fg() -> Color { rgba(0x1e1e2eff) }

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn colors_are_distinct() {
        assert_ne!(bg(), fg());
        assert_ne!(fg(), fg_dim());
        assert_ne!(accent(), danger());
        assert_ne!(btn_bg(), btn_fg());
    }

    #[test]
    fn accent_equals_border_focus() {
        assert_eq!(accent(), border_focus());
    }

    #[test]
    fn bg_equals_btn_fg() {
        assert_eq!(bg(), btn_fg());
    }

    #[test]
    fn bg_hover_equals_border_color() {
        assert_eq!(bg_hover(), border_color());
    }
}
