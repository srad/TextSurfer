use ratatui::buffer::Buffer;
use ratatui::layout::Rect;
use ratatui::style::{Color, Modifier, Style};
use ratatui::widgets::Widget;

use crate::core::style::CellStyle;
use crate::paint::{DisplayList, PaintedSpan};
use crate::ui::theme::Theme;

pub struct ContentLines<'a> {
    pub painted: &'a DisplayList,
    pub scroll: usize,
}

pub struct Content<'a> {
    pub lines: &'a ContentLines<'a>,
    pub theme: &'a Theme,
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
    }
}

fn span_style(span: &PaintedSpan, theme: &Theme) -> Style {
    let CellStyle {
        fg,
        bg,
        bold,
        underline,
        strike,
        reverse,
    } = span.style;
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
    style
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ui::theme::NORTON;
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
                    },
                    theme: &NORTON,
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
                    },
                    theme: &NORTON,
                }
                .render(frame.area(), frame.buffer_mut())
            })
            .unwrap();
        let buffer = terminal.backend().buffer();
        let link = &buffer[(1, 0)];
        assert_eq!(link.fg, ratatui::style::Color::Rgb(255, 255, 0));
        assert_eq!(
            link.bg, NORTON.bg,
            "unset backgrounds fall back to the theme"
        );
        assert!(link.modifier.contains(ratatui::style::Modifier::UNDERLINED));
        let loud = &buffer[(6, 0)];
        assert_eq!(
            loud.fg, NORTON.text,
            "unset foregrounds fall back to the theme"
        );
        assert_eq!(loud.bg, ratatui::style::Color::Rgb(0, 128, 0));
        assert!(loud.modifier.contains(ratatui::style::Modifier::BOLD));
        insta::assert_snapshot!(crate::ui::test_util::styled_buffer_string(buffer));
    }
}
