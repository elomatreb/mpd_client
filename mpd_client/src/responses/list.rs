use std::{slice::Iter, vec::IntoIter};

use mpd_protocol::response::Frame;

use crate::tag::Tag;

/// Response to the [`list`] command.
///
/// [`list`]: crate::commands::definitions::List
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct List<const N: usize> {
    primary_tag: Tag,
    groupings: [Tag; N],
    fields: Vec<(Tag, String)>,
}

impl List<0> {
    /// Returns an iterator over all distinct values returned.
    pub fn values(&self) -> ListValuesIter<'_> {
        ListValuesIter(self.fields.iter())
    }
}

impl<const N: usize> List<N> {
    pub(crate) fn from_frame(primary_tag: Tag, groupings: [Tag; N], frame: Frame) -> List<N> {
        let fields = frame
            .into_iter()
            // Unwrapping here is fine because the parser already validated the fields
            .map(|(tag, value)| (Tag::try_from(tag.as_ref()).unwrap(), value))
            .collect();

        List {
            primary_tag,
            groupings,
            fields,
        }
    }

    /// Returns an iterator over the grouped combinations returned by the command.
    ///
    /// The grouped values are returned in the same order they were passed to [`group_by`].
    ///
    /// [`group_by`]: crate::commands::definitions::List::group_by
    pub fn grouped_values(&self) -> GroupedListValuesIter<'_, N> {
        GroupedListValuesIter {
            primary_tag: &self.primary_tag,
            grouping_tags: &self.groupings,
            grouping_values: [""; N],
            fields: self.fields.iter(),
        }
    }

    /// Returns the tags the response was grouped by.
    pub fn grouped_by(&self) -> &[Tag; N] {
        &self.groupings
    }

    /// Get the raw fields as they were returned by the server.
    pub fn into_raw_values(self) -> Vec<(Tag, String)> {
        self.fields
    }
}

impl<'a> IntoIterator for &'a List<0> {
    type Item = &'a str;

    type IntoIter = ListValuesIter<'a>;

    fn into_iter(self) -> Self::IntoIter {
        self.values()
    }
}

impl IntoIterator for List<0> {
    type Item = String;

    type IntoIter = ListValuesIntoIter;

    fn into_iter(self) -> Self::IntoIter {
        ListValuesIntoIter(self.fields.into_iter())
    }
}

/// Iterator over references to grouped values.
///
/// Returned by [`List::grouped_values`].
#[derive(Clone, Debug)]
pub struct GroupedListValuesIter<'a, const N: usize> {
    primary_tag: &'a Tag,
    grouping_tags: &'a [Tag; N],
    grouping_values: [&'a str; N],
    fields: Iter<'a, (Tag, String)>,
}

impl<'a, const N: usize> Iterator for GroupedListValuesIter<'a, N> {
    type Item = (&'a str, [&'a str; N]);

    fn next(&mut self) -> Option<Self::Item> {
        loop {
            let (tag, value) = self.fields.next()?;

            if tag == self.primary_tag {
                break Some((value, self.grouping_values));
            }

            let idx = self.grouping_tags.iter().position(|t| t == tag).unwrap();
            self.grouping_values[idx] = value;
        }
    }
}

/// Iterator over references to ungrouped [`List`] values.
#[derive(Clone, Debug)]
pub struct ListValuesIter<'a>(Iter<'a, (Tag, String)>);

impl<'a> Iterator for ListValuesIter<'a> {
    type Item = &'a str;

    fn next(&mut self) -> Option<Self::Item> {
        self.0.next().map(|(_, v)| &**v)
    }

    fn size_hint(&self) -> (usize, Option<usize>) {
        self.0.size_hint()
    }

    fn count(self) -> usize {
        self.0.count()
    }

    fn nth(&mut self, n: usize) -> Option<Self::Item> {
        self.0.nth(n).map(|(_, v)| &**v)
    }

    fn last(self) -> Option<Self::Item>
    where
        Self: Sized,
    {
        self.0.last().map(|(_, v)| &**v)
    }
}

impl DoubleEndedIterator for ListValuesIter<'_> {
    fn next_back(&mut self) -> Option<Self::Item> {
        self.0.next_back().map(|(_, v)| &**v)
    }

    fn nth_back(&mut self, n: usize) -> Option<Self::Item> {
        self.0.nth_back(n).map(|(_, v)| &**v)
    }
}

impl ExactSizeIterator for ListValuesIter<'_> {}

/// Iterator over ungrouped [`List`] values.
#[derive(Debug)]
pub struct ListValuesIntoIter(IntoIter<(Tag, String)>);

impl Iterator for ListValuesIntoIter {
    type Item = String;

    fn next(&mut self) -> Option<Self::Item> {
        self.0.next().map(|(_, v)| v)
    }

    fn size_hint(&self) -> (usize, Option<usize>) {
        self.0.size_hint()
    }

    fn count(self) -> usize {
        self.0.count()
    }

    fn nth(&mut self, n: usize) -> Option<Self::Item> {
        self.0.nth(n).map(|(_, v)| v)
    }

    fn last(self) -> Option<Self::Item>
    where
        Self: Sized,
    {
        self.0.last().map(|(_, v)| v)
    }
}

impl DoubleEndedIterator for ListValuesIntoIter {
    fn next_back(&mut self) -> Option<Self::Item> {
        self.0.next_back().map(|(_, v)| v)
    }

    fn nth_back(&mut self, n: usize) -> Option<Self::Item> {
        self.0.nth_back(n).map(|(_, v)| v)
    }
}

impl ExactSizeIterator for ListValuesIntoIter {}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn grouped_iterator() {
        let fields = vec![
            (Tag::AlbumArtist, String::from("Foo")),
            (Tag::Album, String::from("Bar")),
            (Tag::Title, String::from("Title 1")),
            (Tag::Title, String::from("Title 2")),
            (Tag::Album, String::from("Quz")),
            (Tag::Title, String::from("Title 3")),
            (Tag::AlbumArtist, String::from("Asdf")),
            (Tag::Album, String::from("Qwert")),
            (Tag::Title, String::from("Title 4")),
        ];

        let mut iter = GroupedListValuesIter {
            primary_tag: &Tag::Title,
            grouping_tags: &[Tag::Album, Tag::AlbumArtist],
            grouping_values: [""; 2],
            fields: fields.iter(),
        };

        assert_eq!(iter.next(), Some(("Title 1", ["Bar", "Foo"])));
        assert_eq!(iter.next(), Some(("Title 2", ["Bar", "Foo"])));
        assert_eq!(iter.next(), Some(("Title 3", ["Quz", "Foo"])));
        assert_eq!(iter.next(), Some(("Title 4", ["Qwert", "Asdf"])));
        assert_eq!(iter.next(), None);
    }
}
