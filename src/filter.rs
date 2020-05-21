//! Tools for constructing [filter expressions], as used by e.g. the `find` command.
//!
//! [filter expressions]: https://www.musicpd.org/doc/html/protocol.html#filters

use std::borrow::Cow;
use std::ops::Not;

use mpd_protocol::command::{escape_argument, Argument};

use crate::Tag;

/// Special tag which checks *all* tag types.
///
/// Provided here to avoid typos.
///
/// ```
/// use mpd_client::{Tag, filter::{Filter, ANY}};
///
/// // Filter for songs where *any* tag equals the word "foo".
/// Filter::tag(ANY, "foo");
/// ```
pub const ANY: &str = "any";

/// Magic value which checks for the absence of the tag with which it is used.
///
/// Provided here to have more apparent meaning than a simple empty string literal.
///
/// ```
/// use mpd_client::{Tag, filter::{Filter, IS_ABSENT}};
///
/// // Filter for songs without an Artist tag.
/// Filter::tag(Tag::Artist, IS_ABSENT);
/// ```
pub const IS_ABSENT: &str = "";

/// A [filter expression].
///
/// [filter expression]: https://www.musicpd.org/doc/html/protocol.html#filters
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Filter(FilterType);

/// Internal filter variant type
#[derive(Clone, Debug, PartialEq, Eq)]
enum FilterType {
    Tag {
        tag: Tag,
        operator: Operator,
        value: Cow<'static, str>,
    },
    Not(Box<FilterType>),
    And(Vec<FilterType>),
}

impl Filter {
    /// Create a filter which selects on the given [`tag`], using the given [`operator`], for the
    /// given `value`.
    ///
    /// An error is returned when the given `tag` is empty, but `value` may be empty (which results
    /// in the filter only matching if the `tag` is **not** present, see also [`IS_ABSENT`]).
    ///
    /// The magic value [`any`] checks for the value in any tag.
    ///
    /// ```
    /// use mpd_protocol::command::Argument;
    /// use mpd_client::{Tag, filter::{Filter, Operator}};
    ///
    /// assert_eq!(
    ///     Filter::new(Tag::Artist, Operator::Equal, "foo\'s bar\"").render(),
    ///     "(Artist == \"foo\\\'s bar\\\"\")"
    /// );
    /// ```
    ///
    /// [`tag`]: ../enum.Tag.html
    /// [`operator`]: enum.Operator.html
    /// [`IS_ABSENT`]: static.IS_ABSENT.html
    /// [`any`]: static.ANY.html
    pub fn new(tag: Tag, operator: Operator, value: impl Into<Cow<'static, str>>) -> Self {
        Self(FilterType::Tag {
            tag,
            operator,
            value: value.into(),
        })
    }

    /// Create a filter which checks where the given `tag` is equal to the given `value`.
    ///
    /// Shorthand method that always checks for equality.
    ///
    /// ```
    /// use mpd_protocol::command::Argument;
    /// use mpd_client::{Filter, Tag};
    ///
    /// assert_eq!(
    ///     Filter::tag(Tag::Artist, "hello world").render(),
    ///     "(Artist == \"hello world\")"
    /// );
    /// ```
    pub fn tag(tag: Tag, value: impl Into<Cow<'static, str>>) -> Self {
        Filter::new(tag, Operator::Equal, value)
    }

    /// Negate the filter.
    ///
    /// You can also use the negation operator (`!`) if you prefer to negate at the start of an
    /// expression.
    ///
    /// ```
    /// use mpd_protocol::command::Argument;
    /// use mpd_client::{Filter, Tag};
    ///
    /// assert_eq!(
    ///     Filter::tag(Tag::Artist, "hello").negate().render(),
    ///     "(!(Artist == \"hello\"))"
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
    /// use mpd_protocol::command::Argument;
    /// use mpd_client::{Filter, Tag};
    ///
    /// assert_eq!(
    ///     Filter::tag(Tag::Artist, "foo").and(Filter::tag(Tag::Album, "bar")).render(),
    ///     "((Artist == \"foo\") AND (Album == \"bar\"))"
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

impl Not for Filter {
    type Output = Self;

    fn not(self) -> Self::Output {
        self.negate()
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
                tag.as_str(),
                operator.as_str(),
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

impl Operator {
    fn as_str(&self) -> &'static str {
        match self {
            Operator::Equal => "==",
            Operator::NotEqual => "!=",
            Operator::Contain => "contains",
            Operator::Match => "=~",
            Operator::NotMatch => "!~",
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn filter_simple_equal() {
        assert_eq!(
            Filter::tag(Tag::Artist, "foo\'s bar\"").render(),
            "(Artist == \"foo\\\'s bar\\\"\")"
        );
    }

    #[test]
    fn filter_other_operator() {
        assert_eq!(
            Filter::new(Tag::Artist, Operator::Contain, "mep mep").render(),
            "(Artist contains \"mep mep\")"
        );
    }

    #[test]
    fn filter_not() {
        assert_eq!(
            Filter::tag(Tag::Artist, "hello").negate().render(),
            "(!(Artist == \"hello\"))"
        );
    }

    #[test]
    fn filter_and() {
        let first = Filter::tag(Tag::Artist, "hello");
        let second = Filter::tag(Tag::Album, "world");

        assert_eq!(
            first.and(second).render(),
            "((Artist == \"hello\") AND (Album == \"world\"))"
        );
    }

    #[test]
    fn filter_and_multiple() {
        let first = Filter::tag(Tag::Artist, "hello");
        let second = Filter::tag(Tag::Album, "world");
        let third = Filter::tag(Tag::Title, "foo");

        assert_eq!(
            first.and(second).and(third).render(),
            "((Artist == \"hello\") AND (Album == \"world\") AND (Title == \"foo\"))"
        );
    }
}
