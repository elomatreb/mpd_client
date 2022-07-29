use crate::{responses::KeyValuePair, tag::Tag};

/// Response to the [`list`] command.
///
/// [`list`]: crate::commands::definitions::List
#[derive(Clone, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub struct List {
    /// The fields returned by the command, in the order returned by MPD.
    pub fields: Vec<(Tag, String)>,
}

impl List {
    pub(crate) fn from_frame(fields: impl IntoIterator<Item = KeyValuePair>) -> Self {
        let fields = fields
            .into_iter()
            .map(|(tag, value)| (Tag::try_from(tag.as_ref()).unwrap(), value))
            .collect::<Vec<_>>();

        Self { fields }
    }
}
