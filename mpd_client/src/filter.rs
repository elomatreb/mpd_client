//! Tools for constructing [filter expressions], as used by e.g. the [`find`] command.
//!
//! [`find`]: crate::commands::definitions::Find
//! [filter expressions]: https://www.musicpd.org/doc/html/protocol.html#filters

use std::{borrow::Cow, fmt::Write, ops::Not};

use bytes::{BufMut, BytesMut};
use mpd_protocol::command::Argument;

use crate::Tag;

const TAG_IS_ABSENT: &str = "";

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
        value: String,
    },
    Not(Box<FilterType>),
    And(Vec<FilterType>),
}

impl Filter {
    /// Create a filter which selects on the given `tag`, using the given `operator`, for the
    /// given `value`.
    ///
    /// See also [`Tag::any()`].
    pub fn new<V: Into<String>>(tag: Tag, operator: Operator, value: V) -> Self {
        Self(FilterType::Tag {
            tag,
            operator,
            value: value.into(),
        })
    }

    /// Create a filter which checks where the given `tag` is equal to the given `value`.
    ///
    /// Shorthand method that always checks for equality.
    pub fn tag<V: Into<String>>(tag: Tag, value: V) -> Self {
        Filter::new(tag, Operator::Equal, value)
    }

    /// Create a filter which checks for the existence of `tag` (with any value).
    pub fn tag_exists(tag: Tag) -> Self {
        Filter::new(tag, Operator::NotEqual, String::from(TAG_IS_ABSENT))
    }

    /// Create a filter which checks for the absence of `tag`.
    pub fn tag_absent(tag: Tag) -> Self {
        Filter::new(tag, Operator::Equal, String::from(TAG_IS_ABSENT))
    }

    /// Negate the filter.
    ///
    /// You can also use the negation operator (`!`) if you prefer to negate at the start of an
    /// expression.
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

    fn render(&self, buf: &mut BytesMut) {
        buf.put_u8(b'"');
        self.0.render(buf);
        buf.put_u8(b'"');
    }
}

impl Argument for Filter {
    fn render(&self, buf: &mut BytesMut) {
        self.render(buf);
    }
}

impl Not for Filter {
    type Output = Self;

    fn not(self) -> Self::Output {
        self.negate()
    }
}

impl FilterType {
    fn render(&self, buf: &mut BytesMut) {
        match self {
            FilterType::Tag {
                tag,
                operator,
                value,
            } => {
                write!(
                    buf,
                    r#"({} {} \"{}\")"#,
                    tag.as_str(),
                    operator.as_str(),
                    escape_filter_value(value)
                )
                .unwrap();
            }
            FilterType::Not(inner) => {
                buf.put_slice(b"(!");
                inner.render(buf);
                buf.put_u8(b')');
            }
            FilterType::And(inner) => {
                assert!(inner.len() >= 2);

                buf.put_u8(b'(');

                let mut first = true;
                for filter in inner {
                    if first {
                        first = false;
                    } else {
                        buf.put_slice(b" AND ");
                    }

                    filter.render(buf);
                }

                buf.put_u8(b')');
            }
        }
        /*
        match self {
            FilterType::And(inner) => {
                assert!(inner.len() >= 2);
                let inner = inner.iter().map(|s| s.render()).collect::<Vec<_>>();

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
        */
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

fn escape_filter_value(value: &str) -> Cow<'_, str> {
    if value.contains('"') {
        Cow::Owned(value.replace('"', r#"\\""#))
    } else {
        Cow::Borrowed(value)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn filter_escaping() {
        let mut buf = BytesMut::new();

        Filter::tag(Tag::Artist, "foo").render(&mut buf);
        assert_eq!(buf, r#""(Artist == \"foo\")""#);
        buf.clear();

        Filter::tag(Tag::Artist, "foo\'s bar\"").render(&mut buf);
        assert_eq!(buf, r#""(Artist == \"foo's bar\\"\")""#);
        buf.clear();
    }

    #[test]
    fn filter_other_operator() {
        let mut buf = BytesMut::new();
        Filter::new(Tag::Artist, Operator::Contain, "mep mep").render(&mut buf);
        assert_eq!(buf, r#""(Artist contains \"mep mep\")""#);
    }

    #[test]
    fn filter_not() {
        let mut buf = BytesMut::new();
        Filter::tag(Tag::Artist, "hello").negate().render(&mut buf);
        assert_eq!(buf, r#""(!(Artist == \"hello\"))""#);
    }

    #[test]
    fn filter_and() {
        let mut buf = BytesMut::new();

        let first = Filter::tag(Tag::Artist, "hello");
        let second = Filter::tag(Tag::Album, "world");

        first.and(second).render(&mut buf);
        assert_eq!(buf, r#""((Artist == \"hello\") AND (Album == \"world\"))""#);
    }

    #[test]
    fn filter_and_multiple() {
        let mut buf = BytesMut::new();

        let first = Filter::tag(Tag::Artist, "hello");
        let second = Filter::tag(Tag::Album, "world");
        let third = Filter::tag(Tag::Title, "foo");

        first.and(second).and(third).render(&mut buf);
        assert_eq!(
            buf,
            r#""((Artist == \"hello\") AND (Album == \"world\") AND (Title == \"foo\"))""#
        );
    }
}
