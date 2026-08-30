use super::{BorderColor, ComputedStyle, Cursor, StyleTree};

impl StyleTree {
    pub(crate) fn layout_compatible_for(
        &self,
        next: &Self,
        nodes: &[crate::core::dom::NodeId],
    ) -> bool {
        nodes.iter().all(|node| {
            layout_style(self.get(*node)) == layout_style(next.get(*node))
                && [super::PseudoElement::Before, super::PseudoElement::After]
                    .into_iter()
                    .all(
                        |which| match (self.pseudo(*node, which), next.pseudo(*node, which)) {
                            (Some(old), Some(new)) => {
                                old.text == new.text
                                    && layout_style(old.style) == layout_style(new.style)
                            }
                            (None, None) => true,
                            _ => false,
                        },
                    )
                && match (self.marker(*node), next.marker(*node)) {
                    (Some(old), Some(new)) => {
                        old.text == new.text
                            && old.reserve == new.reserve
                            && old.position == new.position
                            && layout_style(old.style) == layout_style(new.style)
                    }
                    (None, None) => true,
                    _ => false,
                }
        })
    }

    pub(crate) fn paint_changes_for(
        &self,
        next: &Self,
        nodes: &[crate::core::dom::NodeId],
    ) -> Vec<(ComputedStyle, ComputedStyle)> {
        let mut changes = Vec::new();
        for node in nodes {
            let old = self.get(*node);
            let new = next.get(*node);
            if old != new {
                changes.push((old, new));
            }
            for which in [super::PseudoElement::Before, super::PseudoElement::After] {
                if let (Some(old), Some(new)) =
                    (self.pseudo(*node, which), next.pseudo(*node, which))
                    && old.style != new.style
                {
                    changes.push((old.style, new.style));
                }
            }
            if let (Some(old), Some(new)) = (self.marker(*node), next.marker(*node))
                && old.style != new.style
            {
                changes.push((old.style, new.style));
            }
        }
        changes
    }

    pub(crate) fn paint_compatible_for(
        &self,
        next: &Self,
        nodes: &[crate::core::dom::NodeId],
    ) -> bool {
        nodes.iter().all(|node| {
            paint_style(self.get(*node)) == paint_style(next.get(*node))
                && [super::PseudoElement::Before, super::PseudoElement::After]
                    .into_iter()
                    .all(
                        |which| match (self.pseudo(*node, which), next.pseudo(*node, which)) {
                            (Some(old), Some(new)) => {
                                old.text == new.text
                                    && paint_style(old.style) == paint_style(new.style)
                            }
                            (None, None) => true,
                            _ => false,
                        },
                    )
                && match (self.marker(*node), next.marker(*node)) {
                    (Some(old), Some(new)) => {
                        old.text == new.text
                            && old.reserve == new.reserve
                            && old.position == new.position
                            && paint_style(old.style) == paint_style(new.style)
                    }
                    (None, None) => true,
                    _ => false,
                }
        })
    }

    pub(crate) fn layout_compatible_with(&self, next: &Self) -> bool {
        self.styles.len() == next.styles.len()
            && self.styles.iter().all(|(node, style)| {
                next.styles
                    .get(node)
                    .is_some_and(|next| layout_style(*style) == layout_style(*next))
            })
            && self.pseudo.len() == next.pseudo.len()
            && self.pseudo.iter().all(|(key, box_)| {
                next.pseudo.get(key).is_some_and(|next| {
                    box_.text == next.text && layout_style(box_.style) == layout_style(next.style)
                })
            })
            && self.markers.len() == next.markers.len()
            && self.markers.iter().all(|(node, marker)| {
                next.markers.get(node).is_some_and(|next| {
                    marker.text == next.text
                        && marker.reserve == next.reserve
                        && marker.position == next.position
                        && layout_style(marker.style) == layout_style(next.style)
                })
            })
    }

    pub(crate) fn paint_changes(&self, next: &Self) -> Vec<(ComputedStyle, ComputedStyle)> {
        let mut changes = self
            .styles
            .iter()
            .filter_map(|(node, style)| next.styles.get(node).map(|next| (*style, *next)))
            .filter(|(style, next)| style != next)
            .collect::<Vec<_>>();
        changes.extend(
            self.pseudo
                .iter()
                .filter_map(|(key, box_)| next.pseudo.get(key).map(|next| (box_.style, next.style)))
                .filter(|(style, next)| style != next),
        );
        changes.extend(
            self.markers
                .iter()
                .filter_map(|(node, marker)| {
                    next.markers
                        .get(node)
                        .map(|next| (marker.style, next.style))
                })
                .filter(|(style, next)| style != next),
        );
        changes
    }

    pub(crate) fn paint_compatible_with(&self, next: &Self) -> bool {
        self.styles.len() == next.styles.len()
            && self.styles.iter().all(|(node, style)| {
                next.styles
                    .get(node)
                    .is_some_and(|next| paint_style(*style) == paint_style(*next))
            })
            && self.pseudo.len() == next.pseudo.len()
            && self.pseudo.iter().all(|(key, box_)| {
                next.pseudo.get(key).is_some_and(|next| {
                    box_.text == next.text && paint_style(box_.style) == paint_style(next.style)
                })
            })
            && self.markers.len() == next.markers.len()
            && self.markers.iter().all(|(node, marker)| {
                next.markers.get(node).is_some_and(|next| {
                    marker.text == next.text
                        && marker.reserve == next.reserve
                        && marker.position == next.position
                        && paint_style(marker.style) == paint_style(next.style)
                })
            })
    }
}

fn layout_style(mut style: ComputedStyle) -> ComputedStyle {
    style.cursor = Cursor::Auto;
    style.color = None;
    style.background = None;
    style.bold = false;
    style.underline = false;
    style.strike = false;
    style.border.top.color = BorderColor::CurrentColor;
    style.border.right.color = BorderColor::CurrentColor;
    style.border.bottom.color = BorderColor::CurrentColor;
    style.border.left.color = BorderColor::CurrentColor;
    style
}

fn paint_style(mut style: ComputedStyle) -> ComputedStyle {
    style.cursor = Cursor::Auto;
    style
}
