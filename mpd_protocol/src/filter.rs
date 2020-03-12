//! Tools for constructing [filter
//! expressions](https://www.musicpd.org/doc/html/protocol.html#filters), as used by e.g. the
//! `find` command.

use std::borrow::Cow;
use std::error::Error;
use std::fmt;

use crate::command::{escape_argument, Argument};

/// Special tag which checks *all* tag types.
///
/// Provided here to avoid typos.
pub static ANY: &str = "any";

/// Magic value which checks for the absence of the tag with which it is used.
///
/// Provided here to have more apparent meaning than a simple empty string literal.
pub static IS_ABSENT: &str = "";

/// A [filter expression](https://www.musicpd.org/doc/html/protocol.html#filters).
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
        tag: Cow<'static, str>,
        operator: Operator,
        value: Cow<'static, str>,
    },
    Not(Box<FilterType>),
    And(Vec<FilterType>),
}

impl Filter {
    /// Create a filter which selects on the given `tag`, using the given
    /// [`operator`](enum.Operator.html), for the given `value`.
    ///
    /// An error is returned when the given `tag` is empty, but `value` may be empty (which results
    /// in the filter only matching if the `tag` is **not** present, see also
    /// [`IS_ABSENT`](static.IS_ABSENT.html)).
    ///
    /// The magic value [`any`](static.ANY.html) checks for the value in any tag.
    ///
    /// ```
    /// use mpd_protocol::command::Argument;
    /// use mpd_protocol::filter::{FilterError, Filter, Operator};
    ///
    /// assert_eq!(
    ///     Filter::tag("artist", Operator::Equal, "foo\'s bar\"").unwrap().render(),
    ///     "(artist == \"foo\\\'s bar\\\"\")"
    /// );
    /// assert_eq!(
    ///     Filter::tag("", Operator::Equal, "").unwrap_err(),
    ///     FilterError::EmptyTag
    /// );
    /// ```
    pub fn tag(
        tag: impl Into<Cow<'static, str>>,
        operator: Operator,
        value: impl Into<Cow<'static, str>>,
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
    /// Similar to [`tag`](#method.tag), but always checks for equality and panics when the given
    /// `tag` is invalid.
    ///
    /// ```
    /// use mpd_protocol::{Filter, command::Argument};
    ///
    /// assert_eq!(
    ///     Filter::equal("artist", "hello world").render(),
    ///     "(artist == \"hello world\")"
    /// );
    /// ```
    pub fn equal(tag: impl Into<Cow<'static, str>>, value: impl Into<Cow<'static, str>>) -> Self {
        Filter::tag(tag, Operator::Equal, value).expect("Invalid filter expression")
    }

    /// Negate the given filter.
    ///
    /// Like [`negate`](#method.negate), but can be used at the start of constructing a filter.
    ///
    /// ```
    /// use mpd_protocol::{Filter, command::Argument};
    ///
    /// assert_eq!(
    ///     Filter::not(Filter::equal("artist", "foo")),
    ///     Filter::equal("artist", "foo").negate()
    /// );
    /// ```
    #[allow(clippy::should_implement_trait)]
    pub fn not(other: Self) -> Self {
        other.negate()
    }

    /// Negate the filter.
    ///
    /// ```
    /// use mpd_protocol::{Filter, command::Argument};
    ///
    /// assert_eq!(
    ///     Filter::equal("artist", "hello").negate().render(),
    ///     "(!(artist == \"hello\"))"
    /// );
    /// ```
    pub fn negate(mut self) -> Self {
        self.0 = FilterType::Not(Box::new(self.0));
        self
    }

    /// Chain the given filter onto this one with an `AND`.
    ///
    /// Automatically flattens nested `AND` conditions.
    ///
    /// ```
    /// use mpd_protocol::{Filter, command::Argument};
    ///
    /// assert_eq!(
    ///     Filter::equal("artist", "foo").and(Filter::equal("album", "bar")).render(),
    ///     "((artist == \"foo\") AND (album == \"bar\"))"
    /// );
    /// ```
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
}

impl Argument for Filter {
    fn render(self) -> Cow<'static, str> {
        Cow::Owned(self.0.render())
    }
}

impl Operator {
    fn to_str(self) -> &'static str {
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
    use super::{Argument, Filter, FilterError, Operator};

    #[test]
    fn filter_simple_equal() {
        assert_eq!(
            Filter::equal("artist", "foo\'s bar\"").render(),
            "(artist == \"foo\\\'s bar\\\"\")"
        );
    }

    #[test]
    fn filter_other_operator() {
        assert_eq!(
            Filter::tag("artist", Operator::Contain, "mep mep")
                .unwrap()
                .render(),
            "(artist contains \"mep mep\")"
        );
    }

    #[test]
    fn filter_empty_value() {
        assert_eq!(
            Filter::tag("", Operator::Equal, "mep mep").unwrap_err(),
            FilterError::EmptyTag,
        );
    }

    #[test]
    fn filter_not() {
        assert_eq!(
            Filter::equal("artist", "hello").negate().render(),
            "(!(artist == \"hello\"))"
        );
    }

    #[test]
    fn filter_and() {
        let first = Filter::equal("artist", "hello");
        let second = Filter::equal("album", "world");

        assert_eq!(
            first.and(second).render(),
            "((artist == \"hello\") AND (album == \"world\"))"
        );
    }

    #[test]
    fn filter_and_multiple() {
        let first = Filter::equal("artist", "hello");
        let second = Filter::equal("album", "world");
        let third = Filter::equal("title", "foo");

        assert_eq!(
            first.and(second).and(third).render(),
            "((artist == \"hello\") AND (album == \"world\") AND (title == \"foo\"))"
        );
    }
}
