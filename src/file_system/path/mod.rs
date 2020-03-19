use crate::file_system::error;
use crate::nonempty::split_last;
use nonempty::NonEmpty;
use std::convert::TryFrom;
use std::fmt;
use std::ops::Deref;
use std::path;
use std::str::FromStr;

pub mod unsound;

/// `Label` is a special case of a `String` identifier for
/// [`Directory`](`crate::file_system::directory::Directory`) and
/// [`File`](`crate::file_system::directory::File`) names, and is used in [`Path`] as the component
/// parts of a path.
///
/// A `Label` should not be empty or contain `/`s. It is encouraged to use the `TryFrom` instance to
/// create a `Label`.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct Label {
    pub(crate) label: String,
    pub(crate) hidden: bool,
}

impl Deref for Label {
    type Target = String;

    fn deref(&self) -> &Self::Target {
        &self.label
    }
}

impl Label {
    /// The root label for the root directory, i.e. `"~"`.
    ///
    /// Prefer creating a root [`Path`], by using [`Path::root`](struct.Path.html#method.root).
    ///
    /// # Examples
    ///
    /// ```
    /// use radicle_surf::file_system::{Label, Path};
    ///
    /// let root = Path::root();
    /// assert_eq!(*root.split_first().0, Label::root());
    /// ```
    pub fn root() -> Self {
        Label {
            label: "~".into(),
            hidden: false,
        }
    }

    /// Check that the label is equivalent to [`Label::root`].
    ///
    /// # Examples
    ///
    /// ```
    /// use radicle_surf::file_system::Label;
    /// use radicle_surf::file_system::unsound;
    ///
    /// let root = unsound::label::new("~");
    /// assert!(root.is_root());
    /// ```
    pub fn is_root(&self) -> bool {
        *self == Self::root()
    }
}

impl fmt::Display for Label {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.label)
    }
}

impl TryFrom<&str> for Label {
    type Error = error::Error;

    fn try_from(item: &str) -> Result<Self, Self::Error> {
        if item.is_empty() {
            Err(error::EMPTY_LABEL)
        } else if item.contains('/') {
            Err(error::label_has_slash(item))
        } else {
            Ok(Label {
                label: item.into(),
                hidden: false,
            })
        }
    }
}

impl FromStr for Label {
    type Err = error::Error;

    fn from_str(item: &str) -> Result<Self, Self::Err> {
        Label::try_from(item)
    }
}

/// A non-empty set of [`Label`]s to define a path to a directory or file.
///
/// `Path` tends to be used for insertion or find operations.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct Path(pub NonEmpty<Label>);

impl fmt::Display for Path {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let (prefix, suffix) = self.split_last();
        for p in prefix {
            write!(f, "{}/", p)?;
        }
        write!(f, "{}", suffix)
    }
}

impl TryFrom<&str> for Path {
    type Error = error::Error;

    fn try_from(item: &str) -> Result<Self, Self::Error> {
        let mut path = Vec::new();

        for label in item.trim_end_matches('/').split('/') {
            let l = Label::try_from(label)?;
            path.push(l);
        }

        NonEmpty::from_slice(&path)
            .ok_or(error::EMPTY_PATH)
            .map(Path)
    }
}

impl FromStr for Path {
    type Err = error::Error;

    fn from_str(item: &str) -> Result<Self, Self::Err> {
        Path::try_from(item)
    }
}

impl From<Path> for Vec<Label> {
    fn from(path: Path) -> Self {
        path.0.into()
    }
}

impl Path {
    /// Create a new `Path` with a single [`Label`].
    pub fn new(label: Label) -> Path {
        Path(NonEmpty::new(label))
    }

    /// The root path is a `Path` made up of the single root label (see:
    /// [`Label::root`](#method.root).
    ///
    /// # Examples
    ///
    /// ```
    /// use radicle_surf::file_system::{Label, Path};
    ///
    /// let root = Path::root();
    /// assert_eq!(*root.split_first().0, Label::root());
    /// ```
    pub fn root() -> Self {
        Path(NonEmpty::new(Label::root()))
    }

    /// Check that this is the root path.
    ///
    /// # Examples
    ///
    /// ```
    /// use radicle_surf::file_system::Path;
    /// use radicle_surf::file_system::unsound;
    /// use std::convert::TryFrom;
    ///
    /// let root = Path::root();
    /// let not_root = unsound::path::new("src/lib.rs");
    ///
    /// assert!(root.is_root());
    /// assert!(!not_root.is_root());
    /// ```
    pub fn is_root(&self) -> bool {
        *self == Self::root()
    }

    /// Append two `Path`s together.
    ///
    /// # Examples
    ///
    /// ```
    /// use radicle_surf::file_system::Path;
    /// use radicle_surf::file_system::unsound;
    /// use std::convert::TryFrom;
    ///
    /// let mut path1 = unsound::path::new("foo/bar");
    /// let mut path2 = unsound::path::new("baz/quux");
    /// path1.append(&mut path2);
    /// let expected = unsound::path::new("foo/bar/baz/quux");
    /// assert_eq!(path1, expected);
    /// ```
    pub fn append(&mut self, path: &mut Self) {
        let mut other = path.0.clone().into();
        self.0.append(&mut other)
    }

    /// Push a new [`Label`] onto the `Path`.
    ///
    /// # Examples
    ///
    /// ```
    /// use radicle_surf::file_system::{Label, Path};
    /// use radicle_surf::file_system::unsound;
    ///
    /// let mut root = Path::root();
    /// root.push(unsound::label::new("src"));
    /// root.push(unsound::label::new("lib.rs"));
    ///
    /// assert_eq!(root, unsound::path::new("~/src/lib.rs"));
    /// ```
    pub fn push(&mut self, label: Label) {
        self.0.push(label)
    }

    /// Pop the [`Label`] from the end of the tail.
    ///
    /// # Examples
    ///
    /// ```
    /// use radicle_surf::file_system::{Label, Path};
    /// use radicle_surf::file_system::unsound;
    ///
    /// let mut root = Path::root();
    /// root.push(unsound::label::new("src"));
    /// root.push(unsound::label::new("lib.rs"));
    ///
    /// assert_eq!(root.pop(), Some(unsound::label::new("lib.rs")));
    /// ```
    pub fn pop(&mut self) -> Option<Label> {
        self.0.pop()
    }

    /// Iterator over the [`Label`]s in the `Path`.
    ///
    /// # Examples
    ///
    /// ```
    /// use radicle_surf::file_system::{Label, Path};
    /// use radicle_surf::file_system::unsound;
    ///
    /// let path = unsound::path::new("~/src/lib.rs");
    /// let mut path_iter = path.iter();
    ///
    /// assert_eq!(path_iter.next(), Some(&Label::root()));
    /// assert_eq!(path_iter.next(), Some(&unsound::label::new("src")));
    /// assert_eq!(path_iter.next(), Some(&unsound::label::new("lib.rs")));
    /// ```
    pub fn iter(&self) -> impl Iterator<Item = &Label> {
        self.0.iter()
    }

    /// Get the first [`Label`] in the `Path` and the rest of the [`Label`]s after it.
    ///
    /// # Examples
    ///
    /// ```
    /// use radicle_surf::file_system::{Label, Path};
    /// use radicle_surf::file_system::unsound;
    ///
    /// let path = unsound::path::new("~/src/lib.rs");
    ///
    /// assert_eq!(
    ///     path.split_first(),
    ///     (&Label::root(), &[unsound::label::new("src"), unsound::label::new("lib.rs")][..])
    /// );
    /// ```
    pub fn split_first(&self) -> (&Label, &[Label]) {
        self.0.split_first()
    }

    /// Get the prefix of the [`Label`]s and the last [`Label`].
    ///
    /// This is useful when the prefix is a directory path and the last label is a file name.
    ///
    /// # Examples
    ///
    /// ```
    /// use radicle_surf::file_system::{Label, Path};
    /// use radicle_surf::file_system::unsound;
    ///
    /// let path = unsound::path::new("~/src/lib.rs");
    /// assert_eq!(path.split_last(), (vec![Label::root(), unsound::label::new("src")], unsound::label::new("lib.rs")));
    /// ```
    ///
    /// ```
    /// use radicle_surf::file_system::{Label, Path};
    /// use radicle_surf::file_system::unsound;
    ///
    /// let path = unsound::path::new("foo/bar/baz");
    /// assert_eq!(
    ///     path.split_last(),
    ///     (vec![unsound::label::new("foo"), unsound::label::new("bar")], unsound::label::new("baz"))
    /// );
    /// ```
    pub fn split_last(&self) -> (Vec<Label>, Label) {
        split_last(&self.0)
    }

    /// Construct a `Path` given at least one [`Label`] followed by 0 or more [`Label`]s.
    ///
    /// # Examples
    ///
    /// ```
    /// use nonempty::NonEmpty;
    /// use radicle_surf::file_system::{Path, Label};
    /// use radicle_surf::file_system::unsound;
    ///
    /// let path = Path::from_labels(
    ///     Label::root(),
    ///     &[unsound::label::new("foo"), unsound::label::new("bar"), unsound::label::new("baz.rs")]
    /// );
    ///
    /// let mut expected = Path::root();
    /// expected.push(unsound::label::new("foo"));
    /// expected.push(unsound::label::new("bar"));
    /// expected.push(unsound::label::new("baz.rs"));
    ///
    /// assert_eq!(path, expected);
    /// let path_vec: Vec<Label> = path.0.into();
    /// assert_eq!(
    ///     path_vec,
    ///     vec![Label::root(), unsound::label::new("foo"), unsound::label::new("bar"),
    ///     unsound::label::new("baz.rs")]
    /// );
    /// ```
    pub fn from_labels(root: Label, labels: &[Label]) -> Path {
        Path((root, labels.to_vec()).into())
    }

    /// Construct a `Path` using [`Label::root`](#method.root) as the head of the `Path.
    ///
    /// # Examples
    ///
    /// ```
    /// use nonempty::NonEmpty;
    /// use radicle_surf::file_system::{Label, Path};
    /// use radicle_surf::file_system::unsound;
    ///
    /// let path = Path::with_root(
    ///     &[unsound::label::new("foo"), unsound::label::new("bar"), unsound::label::new("baz.rs")]
    /// );
    ///
    /// let mut expected = Path::root();
    /// expected.push(unsound::label::new("foo"));
    /// expected.push(unsound::label::new("bar"));
    /// expected.push(unsound::label::new("baz.rs"));
    ///
    /// assert_eq!(path, expected);
    /// let path_vec: Vec<Label> = path.0.into();
    /// assert_eq!(
    ///     path_vec,
    ///     vec![Label::root(), unsound::label::new("foo"), unsound::label::new("bar"),
    ///     unsound::label::new("baz.rs")]
    /// );
    /// ```
    pub fn with_root(labels: &[Label]) -> Path {
        Path::from_labels(Label::root(), labels)
    }
}

impl TryFrom<path::PathBuf> for Path {
    type Error = error::Error;

    fn try_from(path_buf: path::PathBuf) -> Result<Self, Self::Error> {
        let mut path = Path::root();
        for p in path_buf.iter() {
            let p = p.to_str().ok_or_else(|| error::label_invalid_utf8(p))?;
            let l = Label::try_from(p)?;
            path.push(l);
        }

        Ok(path)
    }
}

#[cfg(test)]
mod tests {
    #[cfg(test)]
    mod path {
        use crate::file_system::unsound;

        #[test]
        fn split_last_root_and_foo() {
            let path = unsound::path::new("foo");
            assert_eq!(path.split_last(), (vec![], unsound::label::new("foo")));
        }

        #[test]
        fn split_last_same_labels() {
            // An interesting case for when first == last, but doesn't imply a singleton Path.
            let path = unsound::path::new("foo/bar/foo");
            assert_eq!(
                path.split_last(),
                (
                    vec![unsound::label::new("foo"), unsound::label::new("bar")],
                    unsound::label::new("foo")
                )
            );
        }
    }
}
