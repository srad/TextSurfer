use super::super::{BasicCascade, Cascade};
use crate::core::dom::{Document, ElementNs};
use crate::core::style::{
    Alignment, AlignmentSafety, AxisCellLength, ComputedStyle, ContentAlignment, CssGap,
    CssMaxSize, CssPercentage, CssSize, Display, DisplayOutside, FlexBasis, FlexDirection,
    FlexStyle, FlexWrap, ItemAlignment, PseudoElement,
};
use crate::css::{CssParser, CssparserParser, MediaContext};

fn computed(declarations: &str) -> ComputedStyle {
    let mut document = Document::new();
    let node = document.insert_element(None, "div", ElementNs::Html, vec![]);
    let sheet = CssparserParser.parse(&format!("div {{ {declarations} }}"));
    BasicCascade
        .apply(&[sheet], &document, MediaContext::screen())
        .get(node)
}

fn flex(value: &str) -> FlexStyle {
    computed(&format!("flex:{value}")).flex
}

#[test]
fn flex_and_sizing_properties_compute_atomically() {
    let mut document = Document::new();
    let root = document.insert_element(None, "main", ElementNs::Html, vec![]);
    let item = document.insert_element(Some(root), "div", ElementNs::Html, vec![]);
    let sheet = CssparserParser.parse(
        "main { display:flex;flex-flow:column-reverse wrap;gap:16px 8px;height:64px;min-width:25%;max-height:80px } div { flex:2 3 content;order:-4 }",
    );
    let styles = BasicCascade.apply(&[sheet], &document, MediaContext::screen());
    let container = styles.get(root);
    assert_eq!(container.flex.direction, FlexDirection::ColumnReverse);
    assert_eq!(container.flex.wrap, FlexWrap::Wrap);
    assert_eq!(container.flex.row_gap.cells(), Some(1));
    assert_eq!(container.flex.column_gap.cells(), Some(1));
    assert_eq!(container.height, CssSize::Cells(4));
    assert_eq!(
        container.min_width,
        CssSize::Percent(CssPercentage::new(2_500))
    );
    assert_eq!(container.max_height, CssMaxSize::Cells(5));
    let item = styles.get(item);
    assert_eq!(item.flex.grow.get(), 2.0);
    assert_eq!(item.flex.shrink.get(), 3.0);
    assert_eq!(item.flex.basis, FlexBasis::Content);
    assert_eq!(item.flex.order, -4);
}

#[test]
fn an_invalid_flex_shorthand_leaves_the_prior_value_intact() {
    assert_eq!(flex("none;flex:2 nope"), FlexStyle::none());
}

#[test]
fn flex_direction_wrap_and_flow_accept_every_supported_combination() {
    for (value, expected) in [
        ("row", FlexDirection::Row),
        ("row-reverse", FlexDirection::RowReverse),
        ("column", FlexDirection::Column),
        ("column-reverse", FlexDirection::ColumnReverse),
    ] {
        assert_eq!(
            computed(&format!("flex-direction:{value}")).flex.direction,
            expected,
            "{value}"
        );
    }
    for (value, expected) in [
        ("nowrap", FlexWrap::NoWrap),
        ("wrap", FlexWrap::Wrap),
        ("wrap-reverse", FlexWrap::WrapReverse),
    ] {
        assert_eq!(
            computed(&format!("flex-wrap:{value}")).flex.wrap,
            expected,
            "{value}"
        );
    }
    for direction in ["row", "row-reverse", "column", "column-reverse"] {
        for wrap in ["nowrap", "wrap", "wrap-reverse"] {
            let first = computed(&format!("flex-flow:{direction} {wrap}"));
            let reversed = computed(&format!("flex-flow:{wrap} {direction}"));
            assert_eq!(first.flex, reversed.flex, "{direction} {wrap}");
        }
    }
}

#[test]
fn flex_shorthand_matrix_preserves_each_omission_rule() {
    for (value, grow, shrink, basis) in [
        ("none", 0.0, 0.0, FlexBasis::Auto),
        ("auto", 1.0, 1.0, FlexBasis::Auto),
        ("0", 0.0, 1.0, FlexBasis::zero()),
        ("0px", 1.0, 1.0, FlexBasis::zero()),
        ("2", 2.0, 1.0, FlexBasis::zero()),
        ("2 3", 2.0, 3.0, FlexBasis::zero()),
        (
            "2 10ch",
            2.0,
            1.0,
            FlexBasis::Cells(AxisCellLength {
                horizontal: 10,
                vertical: 5,
            }),
        ),
        (
            "10ch",
            1.0,
            1.0,
            FlexBasis::Cells(AxisCellLength {
                horizontal: 10,
                vertical: 5,
            }),
        ),
        (
            "2 3 25%",
            2.0,
            3.0,
            FlexBasis::Percent(CssPercentage::new(2_500)),
        ),
        ("2 3 content", 2.0, 3.0, FlexBasis::Content),
    ] {
        let actual = flex(value);
        assert_eq!(actual.grow.get(), grow, "{value}");
        assert_eq!(actual.shrink.get(), shrink, "{value}");
        assert_eq!(actual.basis, basis, "{value}");
    }
    assert_eq!(
        flex("none;flex-direction:column").direction,
        FlexDirection::Column
    );
    assert_eq!(
        flex("1 1 auto;align-self:end").align_self.unwrap().keyword,
        ItemAlignment::End
    );
}

#[test]
fn flex_alignment_keywords_and_safety_compute_without_cross_property_leaks() {
    for (value, keyword) in [
        ("normal", ContentAlignment::Normal),
        ("stretch", ContentAlignment::Stretch),
        ("start", ContentAlignment::Start),
        ("end", ContentAlignment::End),
        ("flex-start", ContentAlignment::FlexStart),
        ("flex-end", ContentAlignment::FlexEnd),
        ("center", ContentAlignment::Center),
        ("space-between", ContentAlignment::SpaceBetween),
        ("space-around", ContentAlignment::SpaceAround),
        ("space-evenly", ContentAlignment::SpaceEvenly),
        ("left", ContentAlignment::Left),
        ("right", ContentAlignment::Right),
    ] {
        assert_eq!(
            computed(&format!("justify-content:{value}"))
                .flex
                .justify_content,
            Alignment {
                keyword,
                safety: AlignmentSafety::Unsafe,
            },
            "{value}"
        );
    }
    for (value, keyword) in [
        ("normal", ItemAlignment::Normal),
        ("stretch", ItemAlignment::Stretch),
        ("start", ItemAlignment::Start),
        ("end", ItemAlignment::End),
        ("flex-start", ItemAlignment::FlexStart),
        ("flex-end", ItemAlignment::FlexEnd),
        ("self-start", ItemAlignment::SelfStart),
        ("self-end", ItemAlignment::SelfEnd),
        ("center", ItemAlignment::Center),
        ("baseline", ItemAlignment::Baseline),
    ] {
        assert_eq!(
            computed(&format!("align-items:{value}")).flex.align_items,
            Alignment {
                keyword,
                safety: AlignmentSafety::Unsafe,
            },
            "{value}"
        );
    }
    let safe = computed(
        "justify-content:safe center;align-content:safe center;align-items:unsafe self-end;align-self:safe flex-start",
    );
    assert_eq!(safe.flex.justify_content.safety, AlignmentSafety::Safe);
    assert_eq!(safe.flex.align_content.safety, AlignmentSafety::Safe);
    assert_eq!(safe.flex.align_items.safety, AlignmentSafety::Unsafe);
    assert_eq!(safe.flex.align_self.unwrap().safety, AlignmentSafety::Safe);
}

#[test]
fn invalid_flex_numbers_gaps_and_order_leave_prior_values_intact() {
    let style = computed(
        "flex-grow:2;flex-grow:-1;flex-shrink:3;flex-shrink:-2;order:-7;order:2.5;gap:16px 10%;gap:-1px 2ch;flex-basis:25%;flex-basis:-1px",
    );
    assert_eq!(style.flex.grow.get(), 2.0);
    assert_eq!(style.flex.shrink.get(), 3.0);
    assert_eq!(style.flex.order, -7);
    assert_eq!(style.flex.row_gap, CssGap::Cells(1));
    assert_eq!(
        style.flex.column_gap,
        CssGap::Percent(CssPercentage::new(1_000))
    );
    assert_eq!(
        style.flex.basis,
        FlexBasis::Percent(CssPercentage::new(2_500))
    );
}

#[test]
fn invalid_sizing_values_leave_prior_values_intact() {
    let style = computed(
        "height:50%;height:-1px;min-width:3ch;min-width:-2px;max-height:64px;max-height:-3px",
    );
    assert_eq!(style.height, CssSize::Percent(CssPercentage::new(5_000)));
    assert_eq!(style.min_width, CssSize::Cells(3));
    assert_eq!(style.max_height, CssMaxSize::Cells(4));
}

#[test]
fn every_flex_css_wide_group_inherits_or_resets_as_one_atomic_property() {
    let mut document = Document::new();
    let parent = document.insert_element(None, "main", ElementNs::Html, vec![]);
    let inherited = document.insert_element(Some(parent), "div", ElementNs::Html, vec![]);
    let reset = document.insert_element(Some(parent), "span", ElementNs::Html, vec![]);
    let sheet = CssparserParser.parse(
        "main { display:flex;flex-flow:column wrap-reverse;flex:2 3 25%;order:4;justify-content:center;align-items:end;align-self:start;align-content:space-between;gap:16px 2ch }
         div { flex-flow:inherit;flex:inherit;order:inherit;justify-content:inherit;align-items:inherit;align-self:inherit;align-content:inherit;gap:inherit }
         span { flex-flow:unset;flex:unset;order:unset;justify-content:unset;align-items:unset;align-self:unset;align-content:unset;gap:unset }",
    );
    let styles = BasicCascade.apply(&[sheet], &document, MediaContext::screen());
    assert_eq!(styles.get(inherited).flex, styles.get(parent).flex);
    assert_eq!(styles.get(reset).flex, FlexStyle::default());
}

#[test]
fn gap_and_place_content_cover_one_and_two_value_forms() {
    let normal = computed("gap:normal");
    assert_eq!(normal.flex.row_gap, CssGap::Normal);
    assert_eq!(normal.flex.column_gap, CssGap::Normal);
    let percent = computed("gap:12.5%");
    assert_eq!(
        percent.flex.row_gap,
        CssGap::Percent(CssPercentage::new(1_250))
    );
    assert_eq!(percent.flex.column_gap, percent.flex.row_gap);
    let one = computed("place-content:center");
    assert_eq!(one.flex.align_content.keyword, ContentAlignment::Center);
    assert_eq!(one.flex.justify_content.keyword, ContentAlignment::Center);
    let two = computed("place-content:end left");
    assert_eq!(two.flex.align_content.keyword, ContentAlignment::End);
    assert_eq!(two.flex.justify_content.keyword, ContentAlignment::Left);
}

#[test]
fn invalid_alignment_gap_and_flow_values_leave_prior_values_intact() {
    let invalid = computed(
        "justify-content:center;justify-content:safe space-between;align-items:end;align-items:safe stretch;align-content:flex-end;align-content:safe normal;align-self:end;align-self:auto;gap:1ch;gap:normal junk;flex-flow:column wrap;flex-flow:row row",
    );
    assert_eq!(
        invalid.flex.justify_content.keyword,
        ContentAlignment::Center
    );
    assert_eq!(invalid.flex.align_items.keyword, ItemAlignment::End);
    assert_eq!(
        invalid.flex.align_content.keyword,
        ContentAlignment::FlexEnd
    );
    assert_eq!(invalid.flex.align_self, None);
    assert_eq!(invalid.flex.row_gap, CssGap::Cells(1));
    assert_eq!(invalid.flex.column_gap, CssGap::Cells(1));
    assert_eq!(invalid.flex.direction, FlexDirection::Column);
    assert_eq!(invalid.flex.wrap, FlexWrap::Wrap);
}

#[test]
fn all_sizing_axes_accept_auto_length_percentage_and_none_forms() {
    let style = computed(
        "width:25%;height:48px;min-width:auto;min-height:50%;max-width:7ch;max-height:none",
    );
    assert_eq!(style.width, CssSize::Percent(CssPercentage::new(2_500)));
    assert_eq!(style.height, CssSize::Cells(3));
    assert_eq!(style.min_width, CssSize::Auto);
    assert_eq!(
        style.min_height,
        CssSize::Percent(CssPercentage::new(5_000))
    );
    assert_eq!(style.max_width, CssMaxSize::Cells(7));
    assert_eq!(style.max_height, CssMaxSize::None);
}

#[test]
fn direct_and_contents_flattened_flex_items_are_blockified() {
    let mut document = Document::new();
    let root = document.insert_element(None, "main", ElementNs::Html, vec![]);
    let inline = document.insert_element(Some(root), "span", ElementNs::Html, vec![]);
    let contents = document.insert_element(Some(root), "section", ElementNs::Html, vec![]);
    let nested = document.insert_element(Some(contents), "em", ElementNs::Html, vec![]);
    let internal = document.insert_element(Some(root), "td", ElementNs::Html, vec![]);
    let sheet = CssparserParser.parse(
        "main { display:flex } section { display:contents } main::before { content:'x';display:inline-flex;order:-1 }",
    );
    let styles = BasicCascade.apply(&[sheet], &document, MediaContext::screen());
    assert_eq!(styles.get(inline).display, Display::BLOCK);
    assert_eq!(styles.get(nested).display, Display::BLOCK);
    assert_eq!(styles.get(internal).display, Display::BLOCK);
    let before = styles.pseudo(root, PseudoElement::Before).unwrap();
    assert_eq!(before.style.display, Display::flex(DisplayOutside::Block));
    assert_eq!(before.style.flex.order, -1);
}
