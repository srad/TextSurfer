use ratatui::buffer::Buffer;
use ratatui::layout::Rect;
use ratatui::style::{Color, Modifier, Style};
use ratatui::widgets::Widget;

use crate::core::style::CellStyle;
use crate::paint::{DisplayList, PaintOverlay, PaintedImage, PaintedSpan};
use crate::ui::theme::Theme;
use crate::ui::widgets::text_field::{TextField, TextFieldView};

pub struct ContentLines<'a> {
    pub painted: &'a DisplayList,
    pub scroll: usize,
    pub text_fields: Vec<ContentTextField<'a>>,
}

#[derive(Clone, Copy)]
pub struct ContentTextField<'a> {
    pub rect: crate::layout::LayoutRect,
    pub view: TextFieldView<'a>,
    pub style: CellStyle,
}

pub struct Content<'a> {
    pub lines: &'a ContentLines<'a>,
    pub theme: &'a Theme,
    pub render_images: bool,
}

impl Widget for Content<'_> {
    fn render(self, area: Rect, buf: &mut Buffer) {
        let frame = Style::default().fg(self.theme.frame);
        if area.width < 2 || area.height == 0 {
            return;
        }
        let interior = area.width - 2;
        let start = self.lines.scroll;
        for row in 0..area.height {
            let y = area.y + row;
            buf.set_string(area.x, y, "│", frame);
            buf.set_string(area.right() - 1, y, "│", frame);
            let Some(painted) = self.lines.painted.row(start + usize::from(row)) else {
                continue;
            };
            for span in &painted.spans {
                let Ok(col) = u16::try_from(span.col) else {
                    break;
                };
                if col >= interior {
                    break;
                }
                let clipped = super::clip_width(&span.text, interior - col);
                if clipped.is_empty() {
                    continue;
                }
                buf.set_string(area.x + 1 + col, y, &clipped, span_style(span, self.theme));
            }
        }
        if self.render_images {
            for overlay in &self.lines.painted.overlays {
                let PaintOverlay::Image(index) = *overlay else {
                    continue;
                };
                let Some(placement) = self.lines.painted.images.get(index) else {
                    continue;
                };
                let Some(image) = self.lines.painted.image_assets.get(&placement.asset_id) else {
                    continue;
                };
                if image.revision == placement.revision {
                    render_halfblocks(placement, image, area, self.lines.scroll, self.theme, buf);
                }
            }
        }
        for field in &self.lines.text_fields {
            let Some(row) = field.rect.row.checked_sub(self.lines.scroll) else {
                continue;
            };
            if row >= usize::from(area.height) || field.rect.col >= usize::from(interior) {
                continue;
            }
            let rect = Rect::new(
                area.x + 1 + field.rect.col as u16,
                area.y + row as u16,
                u16::try_from(field.rect.width.min(usize::from(interior) - field.rect.col))
                    .unwrap_or(u16::MAX),
                u16::try_from(
                    field
                        .rect
                        .height
                        .min(usize::from(area.height).saturating_sub(row)),
                )
                .unwrap_or(u16::MAX),
            );
            TextField::new(field.view)
                .style(cell_style(field.style, self.theme))
                .selection_style(self.theme.selected())
                .render(rect, buf);
        }
    }
}

fn render_halfblocks(
    placement: &PaintedImage,
    image: &crate::core::image::DecodedImage,
    area: Rect,
    scroll: usize,
    theme: &Theme,
    buf: &mut Buffer,
) {
    if placement.rect.width == 0 || placement.rect.height == 0 || area.width < 2 {
        return;
    }
    let interior = usize::from(area.width - 2);
    let left = placement.rect.col.max(placement.clip.col);
    let right = placement
        .rect
        .col
        .saturating_add(placement.rect.width)
        .min(placement.clip.col.saturating_add(placement.clip.width))
        .min(interior);
    let top = placement.rect.row.max(placement.clip.row).max(scroll);
    let bottom = placement
        .rect
        .row
        .saturating_add(placement.rect.height)
        .min(placement.clip.row.saturating_add(placement.clip.height))
        .min(scroll.saturating_add(usize::from(area.height)));
    for row in top..bottom {
        for col in left..right {
            let x = area.x.saturating_add(1).saturating_add(col as u16);
            let y = area.y.saturating_add((row - scroll) as u16);
            let background = match buf[(x, y)].bg {
                Color::Rgb(r, g, b) => crate::core::style::Rgb::new(r, g, b),
                _ => crate::ui::theme::rgb_of(theme.bg),
            };
            let top = sampled_color(placement, image, col, row, 0, background);
            let bottom = sampled_color(placement, image, col, row, 1, background);
            buf[(x, y)].set_symbol("▀").set_style(
                Style::default()
                    .fg(Color::Rgb(top.r, top.g, top.b))
                    .bg(Color::Rgb(bottom.r, bottom.g, bottom.b)),
            );
        }
    }
}

fn sampled_color(
    placement: &PaintedImage,
    image: &crate::core::image::DecodedImage,
    col: usize,
    row: usize,
    half: usize,
    background: crate::core::style::Rgb,
) -> crate::core::style::Rgb {
    let local_x = col.saturating_sub(placement.rect.col);
    let local_y = row
        .saturating_sub(placement.rect.row)
        .saturating_mul(2)
        .saturating_add(half);
    let source_x = local_x
        .saturating_mul(image.width as usize)
        .checked_div(placement.rect.width)
        .unwrap_or(0)
        .min(image.width.saturating_sub(1) as usize);
    let source_y = local_y
        .saturating_mul(image.height as usize)
        .checked_div(placement.rect.height.saturating_mul(2))
        .unwrap_or(0)
        .min(image.height.saturating_sub(1) as usize);
    let index = source_y
        .saturating_mul(image.width as usize)
        .saturating_add(source_x)
        .saturating_mul(4);
    image
        .rgba
        .get(index..index.saturating_add(4))
        .map_or(background, |pixel| {
            crate::core::style::Rgba::new(pixel[0], pixel[1], pixel[2], pixel[3])
                .composite_over(background)
        })
}

pub fn span_style(span: &PaintedSpan, theme: &Theme) -> Style {
    cell_style(span.style, theme)
}

fn cell_style(style: CellStyle, theme: &Theme) -> Style {
    let CellStyle {
        fg,
        bg,
        bold,
        underline,
        strike,
        reverse,
        dim,
        scale: _,
    } = style;
    let mut style = Style::default()
        .fg(fg.map_or(theme.text, |color| {
            Color::Rgb(color.rgb.r, color.rgb.g, color.rgb.b)
        }))
        .bg(bg.map_or(theme.bg, |color| Color::Rgb(color.r, color.g, color.b)));
    if bold {
        style = style.add_modifier(Modifier::BOLD);
    }
    if underline {
        style = style.add_modifier(Modifier::UNDERLINED);
    }
    if strike {
        style = style.add_modifier(Modifier::CROSSED_OUT);
    }
    if reverse {
        style = style.add_modifier(Modifier::REVERSED);
    }
    if dim {
        style = style.add_modifier(Modifier::DIM);
    }
    style
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ui::theme::DEFAULT;
    use ratatui::Terminal;
    use ratatui::backend::TestBackend;

    fn render(lines: &[String], scroll: usize, height: u16) -> String {
        let backend = TestBackend::new(24, height);
        let mut terminal = Terminal::new(backend).unwrap();
        terminal
            .draw(|frame| {
                Content {
                    lines: &ContentLines {
                        painted: &DisplayList::from_lines(lines),
                        scroll,
                        text_fields: Vec::new(),
                    },
                    theme: &DEFAULT,
                    render_images: true,
                }
                .render(frame.area(), frame.buffer_mut())
            })
            .unwrap();
        crate::ui::test_util::buffer_string(terminal.backend().buffer())
    }

    fn lines(n: u16) -> Vec<String> {
        (1..=n).map(|i| format!("line {i}")).collect()
    }

    #[test]
    fn renders_the_visible_window() {
        insta::assert_snapshot!(render(&lines(5), 2, 3));
    }

    #[test]
    fn short_documents_render_blank_rows_below() {
        insta::assert_snapshot!(render(&lines(1), 0, 3));
    }

    #[test]
    fn long_lines_are_clipped_to_the_interior() {
        let long = vec!["x".repeat(100)];
        insta::assert_snapshot!(render(&long, 0, 1));
    }

    fn styled_row(spans: Vec<PaintedSpan>) -> DisplayList {
        DisplayList {
            rows: vec![crate::paint::PaintedRow { spans }],
            ..Default::default()
        }
    }

    #[test]
    fn span_colours_and_modifiers_reach_the_terminal_buffer() {
        use crate::core::style::Rgb;
        let painted = styled_row(vec![
            PaintedSpan {
                col: 0,
                text: "link".to_string(),
                style: CellStyle {
                    fg: Some(Rgb::new(255, 255, 0).into()),
                    underline: true,
                    ..Default::default()
                },
            },
            PaintedSpan {
                col: 5,
                text: "loud".to_string(),
                style: CellStyle {
                    bg: Some(Rgb::new(0, 128, 0)),
                    bold: true,
                    ..Default::default()
                },
            },
        ]);
        let backend = TestBackend::new(12, 1);
        let mut terminal = Terminal::new(backend).unwrap();
        terminal
            .draw(|frame| {
                Content {
                    lines: &ContentLines {
                        painted: &painted,
                        scroll: 0,
                        text_fields: Vec::new(),
                    },
                    theme: &DEFAULT,
                    render_images: true,
                }
                .render(frame.area(), frame.buffer_mut())
            })
            .unwrap();
        let buffer = terminal.backend().buffer();
        let link = &buffer[(1, 0)];
        assert_eq!(link.fg, ratatui::style::Color::Rgb(255, 255, 0));
        assert_eq!(
            link.bg, DEFAULT.bg,
            "unset backgrounds fall back to the theme"
        );
        assert!(link.modifier.contains(ratatui::style::Modifier::UNDERLINED));
        let loud = &buffer[(6, 0)];
        assert_eq!(
            loud.fg, DEFAULT.text,
            "unset foregrounds fall back to the theme"
        );
        assert_eq!(loud.bg, ratatui::style::Color::Rgb(0, 128, 0));
        assert!(loud.modifier.contains(ratatui::style::Modifier::BOLD));
        insta::assert_snapshot!(crate::ui::test_util::styled_buffer_string(buffer));
    }

    #[test]
    fn text_field_selection_uses_the_active_theme() {
        let mut state = crate::ui::widgets::text_field::TextFieldState::with_text("ab");
        state.select_all();
        let painted = DisplayList::from_lines(&[String::new()]);
        let backend = TestBackend::new(4, 1);
        let mut terminal = Terminal::new(backend).unwrap();
        terminal
            .draw(|frame| {
                Content {
                    lines: &ContentLines {
                        painted: &painted,
                        scroll: 0,
                        text_fields: vec![ContentTextField {
                            rect: crate::layout::LayoutRect {
                                col: 0,
                                row: 0,
                                width: 2,
                                height: 1,
                            },
                            view: TextFieldView::new(&state),
                            style: CellStyle::default(),
                        }],
                    },
                    theme: &DEFAULT,
                    render_images: true,
                }
                .render(frame.area(), frame.buffer_mut());
            })
            .unwrap();
        let selected = DEFAULT.selected();
        let buffer = terminal.backend().buffer();
        assert_eq!(buffer[(1, 0)].fg, selected.fg.unwrap());
        assert_eq!(buffer[(1, 0)].bg, selected.bg.unwrap());
    }

    #[test]
    fn terminal_images_use_scrollable_alpha_composited_halfblocks() {
        let mut document = crate::core::dom::Document::new();
        let node = document.insert_element(None, "img", crate::core::dom::ElementNs::Html, vec![]);
        let asset_id = crate::core::image::ImageAssetId(7);
        let mut painted = DisplayList {
            rows: vec![crate::paint::PaintedRow::default(); 2],
            images: vec![PaintedImage {
                node,
                asset_id,
                revision: 3,
                rect: crate::layout::LayoutRect {
                    col: 0,
                    row: 1,
                    width: 1,
                    height: 1,
                },
                clip: crate::layout::LayoutRect {
                    col: 0,
                    row: 1,
                    width: 1,
                    height: 1,
                },
                depth: 0,
            }],
            overlays: vec![PaintOverlay::Image(0)],
            ..Default::default()
        };
        painted.image_assets.insert(
            asset_id,
            crate::core::image::DecodedImage {
                asset_id,
                revision: 3,
                width: 1,
                height: 2,
                rgba: std::sync::Arc::from([255, 0, 0, 255, 0, 0, 255, 128]),
                source: crate::core::image::DecodedImageSource::Raster,
            },
        );
        let backend = TestBackend::new(4, 1);
        let mut terminal = Terminal::new(backend).unwrap();
        terminal
            .draw(|frame| {
                Content {
                    lines: &ContentLines {
                        painted: &painted,
                        scroll: 1,
                        text_fields: Vec::new(),
                    },
                    theme: &DEFAULT,
                    render_images: true,
                }
                .render(frame.area(), frame.buffer_mut())
            })
            .unwrap();
        let cell = &terminal.backend().buffer()[(1, 0)];
        assert_eq!(cell.symbol(), "▀");
        assert_eq!(cell.fg, Color::Rgb(255, 0, 0));
        assert_eq!(cell.bg, Color::Rgb(0, 0, 213));
    }
}
