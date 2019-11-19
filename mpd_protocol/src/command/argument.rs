use std::borrow::Cow;
use std::time::Duration;

/// A value that can be used as an argument for a [`Command`](trait.Command.html).
pub(super) trait Argument {
    /// Render the argument to the wire representation
    fn render(self) -> Cow<'static, str>;
}

impl<T> Argument for Vec<T>
where
    T: Argument,
{
    fn render(self) -> Cow<'static, str> {
        let mut first = true;
        // Join the rendered form of the arguments with spaces, but don't emit a space if there are
        let out = self.into_iter().fold(String::new(), |mut acc, argument| {
            if first {
                first = false;
            } else {
                acc.push(' ');
            }

            acc.push_str(&argument.render());
            acc
        });

        Cow::Owned(out)
    }
}

impl Argument for super::SongId {
    fn render(self) -> Cow<'static, str> {
        Cow::Owned(self.to_string())
    }
}

impl Argument for bool {
    fn render(self) -> Cow<'static, str> {
        Cow::Borrowed(if self { "1" } else { "0" })
    }
}

impl Argument for f32 {
    fn render(self) -> Cow<'static, str> {
        Cow::Owned(self.to_string())
    }
}

impl Argument for Duration {
    fn render(self) -> Cow<'static, str> {
        Cow::Owned(if self.subsec_micros() == 0 {
            format!("{}", self.as_secs())
        } else {
            format!("{}.{:06}", self.as_secs(), self.subsec_micros())
        })
    }
}

#[cfg(test)]
mod test {
    use super::*;

    #[test]
    fn argument_render() {
        let none: Vec<bool> = Vec::new();
        let one = vec![true];
        let two = vec![true, false];

        assert_eq!("", none.render());
        assert_eq!("1", one.render());
        assert_eq!("1 0", two.render());
    }

    #[test]
    fn argument_duration() {
        assert_eq!("2", Duration::from_secs(2).render());
        assert_eq!("1.123456", Duration::from_micros(1_123_456).render());
        assert_eq!("0.000001", Duration::from_micros(1).render());
        assert_eq!("0", Duration::from_nanos(999).render());
    }
}
