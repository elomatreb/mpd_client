//! Tools for constructing [filter
//! expressions](https://www.musicpd.org/doc/html/protocol.html#filters), as used by e.g. the
//! `find` command.

use std::error::Error;
use std::fmt;

use crate::command::escape_argument;

/// A filter expression.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Filter(FilterType);

/// Operators which can be used in filter expressions.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Operator {
    /// Equality (`==`)
    Equal,
    /// Negated equality (`!=`)
    NotEqual,
    /// Substring matching (`contains`)
    Contain,
    /// Perl-style regex matching (`=~`)
    Match,
    /// Negated Perl-style regex matching (`!~`)
    NotMatch,
}

/// Error returned when attempting to construct invalid filter expressions.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum FilterError {
    /// Attempted to filter for an empty tag.
    EmptyTag,
}

impl Error for FilterError {}

impl fmt::Display for FilterError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            FilterError::EmptyTag => write!(f, "Attmpted to construct a filter for an empty tag"),
        }
    }
}

/// Internal filter variant type
#[derive(Clone, Debug, PartialEq, Eq)]
enum FilterType {
    Tag {
        tag: String,
        operator: Operator,
        value: String,
    },
    Not(Box<FilterType>),
    And(Vec<FilterType>),
}

impl Filter {
    /// Create a filter which selects on the given `tag`, using the given
    /// [`operator`](enum.Operator.html), for the given `value`.
    ///
    /// An error is returned when the given `tag` is empty, but `value` may be empty (which results
    /// in the filter only matching if the `tag` is **not** present).
    pub fn tag_checked(
        tag: impl Into<String>,
        operator: Operator,
        value: impl Into<String>,
    ) -> Result<Self, FilterError> {
        let tag = tag.into();
        if tag.is_empty() {
            Err(FilterError::EmptyTag)
        } else {
            Ok(Filter(FilterType::Tag {
                tag,
                operator,
                value: value.into(),
            }))
        }
    }

    /// Create a filter which checks where the given `tag` is equal to the given `value`.
    ///
    /// Similar to [`tag_checked`](#method.tag_checked), but always checks for equality and panics
    /// when invalid.
    pub fn tag(tag: impl Into<String>, value: impl Into<String>) -> Self {
        Filter::tag_checked(tag, Operator::Equal, value).expect("Invalid filter expression")
    }

    /// Negate the given filter.
    ///
    /// Like [`negate`](#method.negate), but can be used at the start of constructing a filter.
    pub fn not(other: Self) -> Self {
        other.negate()
    }

    /// Negate the filter.
    pub fn negate(mut self) -> Self {
        self.0 = FilterType::Not(Box::new(self.0));
        self
    }

    /// Chain the given filter onto this one with an `AND`.
    ///
    /// Automatically flattens nested `AND` conditions.
    pub fn and(self, other: Self) -> Self {
        let mut out = match self.0 {
            FilterType::And(inner) => inner,
            condition => {
                let mut out = Vec::with_capacity(2);
                out.push(condition);
                out
            }
        };

        match other.0 {
            FilterType::And(inner) => {
                for c in inner {
                    out.push(c);
                }
            }
            condition => out.push(condition),
        }

        Self(FilterType::And(out))
    }

    /// Render this filter expression to a string ready to be used in a `Command`.
    pub fn render(self) -> String {
        self.0.render()
    }
}

impl Operator {
    fn to_str(&self) -> &'static str {
        match self {
            Operator::Equal => "==",
            Operator::NotEqual => "!=",
            Operator::Contain => "contains",
            Operator::Match => "=~",
            Operator::NotMatch => "!~",
        }
    }
}

impl FilterType {
    fn render(self) -> String {
        match self {
            FilterType::Tag {
                tag,
                operator,
                value,
            } => format!(
                "({} {} \"{}\")",
                tag,
                operator.to_str(),
                escape_argument(&value)
            ),
            FilterType::Not(inner) => format!("(!{})", inner.render()),
            FilterType::And(inner) => {
                assert!(inner.len() >= 2);
                let inner = inner.into_iter().map(|s| s.render()).collect::<Vec<_>>();

                // Wrapping parens
                let mut capacity = 2;
                // Lengths of the actual commands
                capacity += inner.iter().map(|s| s.len()).sum::<usize>();
                // " AND " join operators
                capacity += (inner.len() - 1) * 5;

                let mut out = String::with_capacity(capacity);

                out.push('(');

                let mut first = true;
                for filter in inner {
                    if first {
                        first = false;
                    } else {
                        out.push_str(" AND ");
                    }

                    out.push_str(&filter);
                }

                out.push(')');

                out
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{Filter, FilterError, Operator};

    #[test]
    fn filter_simple_equal() {
        assert_eq!(
            Filter::tag("artist", "foo\'s bar\"")
                .render(),
            "(artist == \"foo\\\'s bar\\\"\")"
        );
    }

    #[test]
    fn filter_other_operator() {
        assert_eq!(
            Filter::tag_checked("artist", Operator::Contain, "mep mep")
                .unwrap()
                .render(),
            "(artist contains \"mep mep\")"
        );
    }

    #[test]
    fn filter_empty_value() {
        assert_eq!(
            Filter::tag_checked("", Operator::Equal, "mep mep").unwrap_err(),
            FilterError::EmptyTag,
        );
    }

    #[test]
    fn filter_not() {
        assert_eq!(
            Filter::tag("artist", "hello")
                .negate()
                .render(),
            "(!(artist == \"hello\"))"
        );
    }

    #[test]
    fn filter_and() {
        let first = Filter::tag("artist", "hello");
        let second = Filter::tag("album", "world");

        assert_eq!(
            first.and(second).render(),
            "((artist == \"hello\") AND (album == \"world\"))"
        );
    }

    #[test]
    fn filter_and_multiple() {
        let first = Filter::tag("artist", "hello");
        let second = Filter::tag("album", "world");
        let third = Filter::tag("title", "foo");

        assert_eq!(
            first.and(second).and(third).render(),
            "((artist == \"hello\") AND (album == \"world\") AND (title == \"foo\"))"
        );
    }
}
