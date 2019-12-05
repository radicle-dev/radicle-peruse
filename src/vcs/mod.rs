use crate::file_system::Directory;
use nonempty::NonEmpty;

pub mod git;

/// A non-empty bag of artifacts which are used to
/// derive a `Directory` view. Examples of artifacts
/// would be commits in Git or patches in Pijul.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub struct History<A>(pub NonEmpty<A>);

impl<A> History<A> {
    /// Iterator over the artifacts.
    pub fn iter<'a>(&'a self) -> impl Iterator<Item = &A> + 'a {
        self.0.iter()
    }

    /// Given that the `History` is topological order from most
    /// recent artifact to least recent, `find_suffix` gets returns
    /// the history up until the point of the given artifact.
    ///
    /// This operation may fail if the artifact does not exist in
    /// the given `History`.
    pub fn find_suffix(&self, artifact: &A) -> Option<Self>
    where
        A: Clone + PartialEq,
    {
        let new_history: Option<NonEmpty<A>> = NonEmpty::from_slice(
            &self
                .iter()
                .cloned()
                .skip_while(|current| *current != *artifact)
                .collect::<Vec<_>>(),
        );

        new_history.map(History)
    }

    pub fn map<F, B>(&self, f: F) -> History<B>
    where
        F: Fn(&A) -> B,
    {
        History(self.0.map(f))
    }

    pub fn find<F, B>(&self, f: F) -> Option<B>
    where
        F: Fn(&A) -> Option<B>,
    {
        self.iter().find_map(f)
    }

    /// Find an atrifact in the given `History` using the artifacts ID.
    ///
    /// This operation may fail if the artifact does not exist in the history.
    pub fn find_in_history<Identifier, F>(&self, identifier: &Identifier, id_of: F) -> Option<A>
    where
        A: Clone,
        F: Fn(&A) -> Identifier,
        Identifier: PartialEq,
    {
        self.iter()
            .find(|artifact| {
                let current_id = id_of(&artifact);
                *identifier == current_id
            })
            .cloned()
    }

    /// Find all occurences of an artifact in a bag of `History`s.
    pub fn find_in_histories<Identifier, F>(
        histories: Vec<Self>,
        identifier: &Identifier,
        id_of: F,
    ) -> Vec<Self>
    where
        A: Clone,
        F: Fn(&A) -> Identifier + Copy,
        Identifier: PartialEq,
    {
        histories
            .into_iter()
            .filter(|history| history.find_in_history(identifier, id_of).is_some())
            .collect()
    }
}

/// A Snapshot is a function that renders a `Directory` given
/// the `Repo` object and a `History` of artifacts.
type Snapshot<'browser, A, Repo, Error> =
    Box<dyn Fn(&Repo, &History<A>) -> Result<Directory, Error> + 'browser>;

/// A `Browser` is a way of rendering a `History` into a
/// `Directory` snapshot, and the current `History` it is
/// viewing.
pub struct Browser<'browser, Repo, A, Error> {
    snapshot: Snapshot<'browser, A, Repo, Error>,
    history: History<A>,
    repository: &'browser Repo,
}

impl<'browser, Repo, A, Error> Browser<'browser, Repo, A, Error> {
    /// Get the current `History` the `Browser` is viewing.
    pub fn get_history(&self) -> History<A>
    where
        A: Clone,
    {
        self.history.clone()
    }

    /// Set the `History` the `Browser` should view.
    pub fn set_history(&mut self, history: History<A>) {
        self.history = history;
    }

    /// Render the `Directory` for this `Browser`.
    pub fn get_directory(&self) -> Result<Directory, Error> {
        (self.snapshot)(&self.repository, &self.history)
    }

    /// Modify the `History` in this `Browser`.
    pub fn modify_history<F>(&mut self, f: F)
    where
        F: Fn(&History<A>) -> History<A>,
    {
        self.history = f(&self.history)
    }

    /// Change the `Browser`'s view of `History` by modifying it, or
    /// using the default `History` provided if the operation fails.
    pub fn view_at<F>(&mut self, default_history: History<A>, f: F)
    where
        A: Clone,
        F: Fn(&History<A>) -> Option<History<A>>,
    {
        self.modify_history(|history| f(history).unwrap_or_else(|| default_history.clone()))
    }
}

impl<'browser, Repo, A, Error> VCS<'browser, A, Error> for Browser<'browser, Repo, A, Error>
where
    A: 'browser,
    Error: 'browser,
    Repo: VCS<'browser, A, Error>,
{
    type HistoryId = Repo::HistoryId;
    type ArtefactId = Repo::ArtefactId;

    fn get_history(&'browser self, identifier: Self::HistoryId) -> Result<History<A>, Error> {
        self.repository.get_history(identifier)
    }

    fn get_histories(&'browser self) -> Result<Vec<History<A>>, Error> {
        self.repository.get_histories()
    }

    fn get_identifier(artifact: &'browser A) -> Self::ArtefactId {
        Repo::get_identifier(artifact)
    }
}

pub(crate) trait GetVCS<'repo, Error>
where
    Self: Sized,
{
    /// The way to identify a Repository.
    type RepoId;

    /// Find a Repository
    fn get_repo(identifier: Self::RepoId) -> Result<Self, Error>;
}

pub trait VCS<'repo, A: 'repo, Error>
where
    Self: 'repo + Sized,
{
    /// The way to identify a History.
    type HistoryId;

    /// The way to identify an artifact.
    type ArtefactId;

    /// Find a History in a Repo given a way to identify it
    fn get_history(&'repo self, identifier: Self::HistoryId) -> Result<History<A>, Error>;

    /// Find all histories in a Repo
    fn get_histories(&'repo self) -> Result<Vec<History<A>>, Error>;

    /// Identify artifacts of a Repository
    fn get_identifier(artifact: &'repo A) -> Self::ArtefactId;
}
