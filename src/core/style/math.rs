use std::collections::HashMap;
use std::sync::Arc;

const MAX_STORED_NODES: usize = 65_536;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct CssCalc(u32);

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub(crate) struct Linear {
    length: u32,
    percent: u32,
}

impl Linear {
    fn new(length: f32, percent: f32) -> Option<Self> {
        if !length.is_finite() || !percent.is_finite() {
            return None;
        }
        Some(Self {
            length: length.to_bits(),
            percent: percent.to_bits(),
        })
    }

    fn length(self) -> f32 {
        f32::from_bits(self.length)
    }

    fn percent(self) -> f32 {
        f32::from_bits(self.percent)
    }

    fn resolve(self, basis: f32) -> Option<f32> {
        finite(self.length() + self.percent() * basis)
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub(crate) enum CssCalcExpr {
    Linear(Linear),
    Sum(Vec<(bool, Self)>),
    Scale {
        value: Box<Self>,
        factor: u32,
    },
    Min(Vec<Self>),
    Max(Vec<Self>),
    Clamp {
        minimum: Option<Box<Self>>,
        value: Box<Self>,
        maximum: Option<Box<Self>>,
    },
}

impl CssCalcExpr {
    pub(crate) fn linear(length: f32, percent: f32) -> Option<Self> {
        Linear::new(length, percent).map(Self::Linear)
    }

    pub(crate) fn add(self, right: Self, positive: bool) -> Option<Self> {
        if let (Self::Linear(left), Self::Linear(right)) = (&self, &right) {
            let sign = if positive { 1.0 } else { -1.0 };
            return Self::linear(
                left.length() + sign * right.length(),
                left.percent() + sign * right.percent(),
            );
        }
        let mut values = match self {
            Self::Sum(values) => values,
            value => vec![(true, value)],
        };
        values.push((positive, right));
        Some(Self::Sum(values))
    }

    pub(crate) fn scale(self, factor: f32) -> Option<Self> {
        if !factor.is_finite() {
            return None;
        }
        match self {
            Self::Linear(value) => Self::linear(value.length() * factor, value.percent() * factor),
            Self::Scale { value, factor: old } => {
                let combined = f32::from_bits(old) * factor;
                combined.is_finite().then_some(Self::Scale {
                    value,
                    factor: combined.to_bits(),
                })
            }
            value if factor == 1.0 => Some(value),
            _ if factor == 0.0 => Self::linear(0.0, 0.0),
            value => Some(Self::Scale {
                value: Box::new(value),
                factor: factor.to_bits(),
            }),
        }
    }

    pub(crate) fn minimum(values: Vec<Self>) -> Option<Self> {
        match values.len() {
            0 => None,
            1 => values.into_iter().next(),
            _ => Some(Self::Min(values)),
        }
    }

    pub(crate) fn maximum(values: Vec<Self>) -> Option<Self> {
        match values.len() {
            0 => None,
            1 => values.into_iter().next(),
            _ => Some(Self::Max(values)),
        }
    }

    pub(crate) fn clamp(minimum: Option<Self>, value: Self, maximum: Option<Self>) -> Self {
        if minimum.is_none() && maximum.is_none() {
            value
        } else {
            Self::Clamp {
                minimum: minimum.map(Box::new),
                value: Box::new(value),
                maximum: maximum.map(Box::new),
            }
        }
    }

    fn resolve(&self, basis: f32) -> Option<f32> {
        match self {
            Self::Linear(value) => value.resolve(basis),
            Self::Sum(values) => values.iter().try_fold(0.0, |total, (positive, value)| {
                let value = value.resolve(basis)?;
                finite(if *positive {
                    total + value
                } else {
                    total - value
                })
            }),
            Self::Scale { value, factor } => {
                finite(value.resolve(basis)? * f32::from_bits(*factor))
            }
            Self::Min(values) => values.iter().try_fold(f32::INFINITY, |result, value| {
                finite(result.min(value.resolve(basis)?))
            }),
            Self::Max(values) => values.iter().try_fold(f32::NEG_INFINITY, |result, value| {
                finite(result.max(value.resolve(basis)?))
            }),
            Self::Clamp {
                minimum,
                value,
                maximum,
            } => {
                let mut result = value.resolve(basis)?;
                if let Some(maximum) = maximum {
                    result = result.min(maximum.resolve(basis)?);
                }
                if let Some(minimum) = minimum {
                    result = minimum.resolve(basis)?.max(result);
                }
                finite(result)
            }
        }
    }

    fn node_count(&self) -> usize {
        match self {
            Self::Linear(_) => 1,
            Self::Sum(values) => {
                1 + values
                    .iter()
                    .map(|(_, value)| value.node_count())
                    .sum::<usize>()
            }
            Self::Scale { value, .. } => 1 + value.node_count(),
            Self::Min(values) | Self::Max(values) => {
                1 + values.iter().map(Self::node_count).sum::<usize>()
            }
            Self::Clamp {
                minimum,
                value,
                maximum,
            } => {
                1 + minimum.as_deref().map_or(0, Self::node_count)
                    + value.node_count()
                    + maximum.as_deref().map_or(0, Self::node_count)
            }
        }
    }
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub(crate) struct CssCalcStore {
    expressions: Vec<Arc<CssCalcExpr>>,
    ids: HashMap<Arc<CssCalcExpr>, CssCalc>,
    nodes: usize,
}

impl CssCalcStore {
    pub(crate) fn insert(&mut self, expression: CssCalcExpr) -> Option<CssCalc> {
        if let Some(value) = self.ids.get(&expression) {
            return Some(*value);
        }
        let nodes = expression.node_count();
        if self.nodes.saturating_add(nodes) > MAX_STORED_NODES {
            return None;
        }
        let value = CssCalc(u32::try_from(self.expressions.len()).ok()?);
        let expression = Arc::new(expression);
        self.expressions.push(expression.clone());
        self.ids.insert(expression, value);
        self.nodes += nodes;
        Some(value)
    }

    pub(crate) fn resolve(&self, value: CssCalc, basis: f32) -> Option<f32> {
        self.expressions.get(value.0 as usize)?.resolve(basis)
    }
}

fn finite(value: f32) -> Option<f32> {
    value.is_finite().then_some(value)
}
