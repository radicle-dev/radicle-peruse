//! ```
//! use nonempty::NonEmpty;
//! use radicle_surf::file_system::{Directory, File, Label, Path, SystemType};
//! use radicle_surf::file_system::unsound;
//! use radicle_surf::vcs::git::*;
//! use std::collections::HashMap;
//!
//! let repo = Repository::new("./data/git-platinum")
//!     .expect("Could not retrieve ./data/git-platinum as git repository");
//! let browser = Browser::new(repo).expect("Could not initialise Browser");
//! let directory = browser.get_directory().expect("Could not render Directory");
//! let mut directory_contents = directory.list_directory();
//! directory_contents.sort();
//!
//! assert_eq!(directory_contents, vec![
//!     SystemType::file(unsound::label::new(".i-am-well-hidden")),
//!     SystemType::file(unsound::label::new(".i-too-am-hidden")),
//!     SystemType::file(unsound::label::new("README.md")),
//!     SystemType::directory(unsound::label::new("bin")),
//!     SystemType::directory(unsound::label::new("src")),
//!     SystemType::directory(unsound::label::new("text")),
//!     SystemType::directory(unsound::label::new("this")),
//! ]);
//!
//! // find src directory in the Git directory and the in-memory directory
//! let src_directory = directory
//!     .find_directory(&Path::new(unsound::label::new("src")))
//!     .unwrap();
//! let mut src_directory_contents = src_directory.list_directory();
//! src_directory_contents.sort();
//!
//! assert_eq!(src_directory_contents, vec![
//!     SystemType::file(unsound::label::new("Eval.hs")),
//!     SystemType::file(unsound::label::new("Folder.svelte")),
//!     SystemType::file(unsound::label::new("memory.rs")),
//! ]);
//! ```

// Re-export git2 as sub-module
pub use git2;
pub use git2::{BranchType, Error as Git2Error, Oid, Time};

pub mod error;
mod object;

use crate::file_system;
use crate::file_system::directory;
use crate::tree::*;
use crate::vcs;
use crate::vcs::git::error::*;
pub use crate::vcs::git::object::*;
use crate::vcs::VCS;
use nonempty::NonEmpty;
use std::cmp::Ordering;
use std::collections::HashMap;
use std::convert::TryFrom;
use std::str;

/// A `History` that uses `git2::Commit` as the underlying artifact.
pub type History = vcs::History<Commit>;

/// Wrapper around the `git2`'s `git2::Repository` type.
/// This is to to limit the functionality that we can do
/// on the underlying object.
pub struct Repository(pub(crate) git2::Repository);

/// OrderedCommit is to allow for us to identify an ordering of commit history as we enumerate over
/// a revwalk of commits, by assigning each commit an identifier.
#[derive(Clone)]
struct OrderedCommit {
    id: usize,
    commit: Commit,
}

impl std::fmt::Debug for OrderedCommit {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "OrderedCommit {{ id: {}, commit: {} }}",
            self.id, self.commit.id
        )
    }
}

impl OrderedCommit {
    fn compare_by_id(&self, other: &Self) -> Ordering {
        self.id.cmp(&other.id).reverse()
    }
}

impl From<OrderedCommit> for Commit {
    fn from(ordered_commit: OrderedCommit) -> Self {
        ordered_commit.commit
    }
}

impl<'repo> Repository {
    /// Open a git repository given its URI.
    ///
    /// # Errors
    ///
    /// * [`error::Error::Git`]
    pub fn new(repo_uri: &str) -> Result<Self, Error> {
        git2::Repository::open(repo_uri)
            .map(Repository)
            .map_err(Error::from)
    }

    /// List the branches within a repository, filtering out ones that do not parse correctly.
    ///
    /// # Errors
    ///
    /// * [`error::Error::Git`]
    pub fn list_branches(&self, filter: Option<BranchType>) -> Result<Vec<Branch>, Error> {
        self.0
            .branches(filter)
            .map_err(Error::from)
            .and_then(|mut branches| {
                branches.try_fold(vec![], |mut acc, branch| {
                    let (branch, branch_type) = branch?;
                    let name = BranchName::try_from(branch.name_bytes()?)?;
                    let branch = Branch {
                        name,
                        locality: branch_type,
                    };
                    acc.push(branch);
                    Ok(acc)
                })
            })
    }

    /// List the tags within a repository, filtering out ones that do not parse correctly.
    ///
    /// # Errors
    ///
    /// * [`error::Error::Git`]
    pub fn list_tags(&self) -> Result<Vec<TagName>, Error> {
        let tags = self.0.tag_names(None)?;
        Ok(tags
            .into_iter()
            .filter_map(|tag| tag.map(TagName::new))
            .collect())
    }

    /// Create a [`RevObject`] given a
    /// [`revspec`](https://git-scm.com/docs/git-rev-parse#_specifying_revisions) string.
    ///
    /// # Errors
    ///
    /// * [`error::Error::Git`]
    /// * [`error::Error::RevParseFailure`]
    pub fn rev(&self, spec: &str) -> Result<RevObject, Error> {
        RevObject::from_revparse(&self.0, spec)
    }

    /// Create a [`History`] given a
    /// [`revspec`](https://git-scm.com/docs/git-rev-parse#_specifying_revisions) string.
    ///
    /// # Errors
    ///
    /// * [`error::Error::Git`]
    /// * [`error::Error::RevParseFailure`]
    pub fn revspec(&self, spec: &str) -> Result<History, Error> {
        let rev = self.rev(spec)?;
        let commit = rev.into_commit(&self.0)?;
        self.commit_to_history(commit)
    }

    /// Get a particular `Commit`.
    pub(crate) fn get_commit(&'repo self, oid: Oid) -> Result<git2::Commit<'repo>, Error> {
        let commit = self.0.find_commit(oid)?;
        Ok(commit)
    }

    /// Build a [`History`] using the `head` reference.
    pub(crate) fn head(&'repo self) -> Result<History, Error> {
        let head = self.0.head()?;
        self.to_history(&head)
    }

    /// Turn a [`git2::Reference`] into a [`History`] by completing
    /// a revwalk over the first commit in the reference.
    pub(crate) fn to_history(
        &'repo self,
        history: &git2::Reference<'repo>,
    ) -> Result<History, Error> {
        let head = history.peel_to_commit()?;
        self.commit_to_history(head)
    }

    /// Turn a [`git2::Reference`] into a [`History`] by completing
    /// a revwalk over the first commit in the reference.
    pub(crate) fn commit_to_history(&'repo self, head: git2::Commit) -> Result<History, Error> {
        let head_id = head.id();
        let mut commits = NonEmpty::new(Commit::try_from(head)?);
        let mut revwalk = self.0.revwalk()?;

        // Set the revwalk to the head commit
        revwalk.push(head_id)?;

        for commit_result_id in revwalk {
            // The revwalk iter returns results so
            // we unpack these and push them to the history
            let commit_id: Oid = commit_result_id?;

            // Skip the head commit since we have processed it
            if commit_id == head_id {
                continue;
            }

            let commit = Commit::try_from(self.0.find_commit(commit_id)?)?;
            commits.push(commit);
        }

        Ok(vcs::History(commits))
    }

    /// Get the history of the file system where the head of the [`NonEmpty`] is the latest commit.
    fn file_history(
        &'repo self,
        commit: Commit,
    ) -> Result<Forest<file_system::Label, NonEmpty<OrderedCommit>>, Error> {
        let mut file_histories = Forest::root();
        self.collect_file_history(&commit.id, &mut file_histories)?;
        Ok(file_histories)
    }

    fn collect_file_history(
        &'repo self,
        commit_id: &Oid,
        file_histories: &mut Forest<file_system::Label, NonEmpty<OrderedCommit>>,
    ) -> Result<(), Error> {
        let mut revwalk = self.0.revwalk()?;

        // Set the revwalk to the head commit
        revwalk.push(commit_id.clone())?;

        for (id, commit_result) in revwalk.enumerate() {
            let parent_id = commit_result?;

            let parent = self.0.find_commit(parent_id)?;
            let paths = self.diff_commit_and_parents(&parent)?;
            let parent_commit = Commit::try_from(parent)?;
            for path in paths {
                let parent_commit = OrderedCommit {
                    id,
                    commit: parent_commit.clone(),
                };

                file_histories.insert_with(
                    &path.0,
                    NonEmpty::new(parent_commit.clone()),
                    |commits| commits.push(parent_commit),
                );
            }
        }
        Ok(())
    }

    fn diff_commit_and_parents(
        &'repo self,
        commit: &'repo git2::Commit,
    ) -> Result<Vec<file_system::Path>, Error> {
        let mut parents = commit.parents();
        let head = parents.next();
        let mut touched_files = vec![];

        let mut add_deltas = |diff: git2::Diff| -> Result<(), Error> {
            let deltas = diff.deltas();

            for delta in deltas {
                let new = delta.new_file().path().ok_or(Error::LastCommitException)?;
                let path = file_system::Path::try_from(new.to_path_buf())?;
                touched_files.push(path);
            }

            Ok(())
        };

        match head {
            None => {
                let diff = self.diff_commits(&commit, None)?;
                add_deltas(diff)?;
            },
            Some(parent) => {
                let diff = self.diff_commits(&commit, Some(&parent))?;
                add_deltas(diff)?;

                for parent in parents {
                    let diff = self.diff_commits(&commit, Some(&parent))?;
                    add_deltas(diff)?;
                }
            },
        }

        Ok(touched_files)
    }

    fn diff_commits(
        &'repo self,
        left: &'repo git2::Commit,
        right: Option<&'repo git2::Commit>,
    ) -> Result<git2::Diff, Error> {
        let left_tree = left.tree()?;
        let right_tree = right.map_or(Ok(None), |commit| commit.tree().map(Some))?;

        let diff = self
            .0
            .diff_tree_to_tree(Some(&left_tree), right_tree.as_ref(), None)?;

        Ok(diff)
    }
}

impl vcs::GetVCS<Error> for Repository {
    type RepoId = String;

    fn get_repo(repo_id: Self::RepoId) -> Result<Self, Error> {
        git2::Repository::open(&repo_id)
            .map(Repository)
            .map_err(Error::from)
    }
}

impl From<git2::Repository> for Repository {
    fn from(repo: git2::Repository) -> Self {
        Repository(repo)
    }
}

impl VCS<Commit, Error> for Repository {
    type HistoryId = String;
    type ArtefactId = Oid;

    fn get_history(&self, history_id: Self::HistoryId) -> Result<History, Error> {
        self.revspec(&history_id)
    }

    fn get_histories(&self) -> Result<Vec<History>, Error> {
        self.0
            .references()
            .map_err(Error::from)
            .and_then(|mut references| {
                references.try_fold(vec![], |mut acc, reference| {
                    reference.map_err(Error::from).and_then(|r| {
                        let history = self.to_history(&r)?;
                        acc.push(history);
                        Ok(acc)
                    })
                })
            })
    }

    fn get_identifier(artifact: &Commit) -> Self::ArtefactId {
        artifact.id
    }
}

impl std::fmt::Debug for Repository {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, ".git")
    }
}

/// A [`crate::vcs::Browser`] that uses [`Repository`] as the underlying repository backend,
/// [`git2::Commit`] as the artifact, and [`Error`] for error reporting.
pub type Browser = vcs::Browser<Repository, Commit, Error>;

impl Browser {
    /// Create a new browser to interact with.
    ///
    /// It uses the current `HEAD` as the starting [`History`].
    ///
    /// # Errors
    ///
    /// * [`error::Error::Git`]
    ///
    /// # Examples
    ///
    /// ```
    /// use radicle_surf::vcs::git::{Browser, Repository};
    ///
    /// let repo = Repository::new("./data/git-platinum").unwrap();
    /// let browser = Browser::new(repo).unwrap();
    /// ```
    pub fn new(repository: Repository) -> Result<Self, Error> {
        let history = repository.head()?;
        let snapshot = Box::new(|repository: &Repository, history: &History| {
            let tree = Self::get_tree(&repository.0, history.0.first())?;
            Ok(directory::Directory::from_hash_map(tree))
        });
        Ok(vcs::Browser {
            snapshot,
            history,
            repository,
        })
    }

    /// Create a new browser to interact with.
    ///
    /// It uses the branch supplied as the starting `History`.
    /// If the branch does not exist an error will be returned.
    ///
    /// # Errors
    ///
    /// * [`error::Error::Git`]
    ///
    /// # Examples
    ///
    /// ```
    /// use radicle_surf::vcs::git::{Browser, Repository};
    ///
    /// let repo = Repository::new("./data/git-platinum").unwrap();
    /// let first_branch = repo.list_branches(None).unwrap().first().cloned().unwrap();
    /// let browser = Browser::new_with_branch(repo, first_branch.name).unwrap();
    /// ```
    pub fn new_with_branch(repository: Repository, branch_name: BranchName) -> Result<Self, Error> {
        let history = repository.get_history(branch_name.name().to_string())?;
        let snapshot = Box::new(|repository: &Repository, history: &History| {
            let tree = Self::get_tree(&repository.0, history.0.first())?;
            Ok(directory::Directory::from_hash_map(tree))
        });
        Ok(vcs::Browser {
            snapshot,
            history,
            repository,
        })
    }

    /// Set the current `Browser` history to the `HEAD` commit of the underlying repository.
    ///
    /// # Errors
    ///
    /// * [`error::Error::Git`]
    ///
    /// # Examples
    ///
    /// ```
    /// use radicle_surf::vcs::git::{Browser, Repository};
    ///
    /// let repo = Repository::new("./data/git-platinum").unwrap();
    /// let mut browser = Browser::new(repo).unwrap();
    ///
    /// // ensure we're at HEAD
    /// browser.head();
    ///
    /// let directory = browser.get_directory();
    ///
    /// // We are able to render the directory
    /// assert!(directory.is_ok());
    /// ```
    pub fn head(&mut self) -> Result<(), Error> {
        let history = self.repository.head()?;
        self.set(history);
        Ok(())
    }

    /// Set the current `Browser`'s [`History`] to the given [`BranchName`] provided.
    ///
    /// # Errors
    ///
    /// * [`error::Error::Git`]
    /// * [`error::Error::NotBranch`]
    ///
    /// # Examples
    ///
    /// ```
    /// use radicle_surf::vcs::git::{BranchName, Browser, Repository};
    ///
    /// let repo = Repository::new("./data/git-platinum").unwrap();
    /// let mut browser = Browser::new(repo).unwrap();
    ///
    /// // ensure we're on 'master'
    /// browser.branch(BranchName::new("master"));
    ///
    /// let directory = browser.get_directory();
    ///
    /// // We are able to render the directory
    /// assert!(directory.is_ok());
    /// ```
    ///
    /// ```
    /// use radicle_surf::vcs::git::{BranchName, Browser, Repository};
    /// use radicle_surf::file_system::{Label, Path, SystemType};
    /// use radicle_surf::file_system::unsound;
    ///
    /// let repo = Repository::new("./data/git-platinum").unwrap();
    /// let mut browser = Browser::new(repo).unwrap();
    /// browser
    ///     .branch(BranchName::new("origin/dev"))
    ///     .expect("Failed to change branch to dev");
    ///
    /// let directory = browser.get_directory().expect("Failed to get directory");
    /// let mut directory_contents = directory.list_directory();
    /// directory_contents.sort();
    ///
    /// assert!(directory_contents.contains(
    ///     &SystemType::file(unsound::label::new("here-we-are-on-a-dev-branch.lol"))
    /// ));
    /// ```
    pub fn branch(&mut self, branch_name: BranchName) -> Result<(), Error> {
        let name = branch_name.name();
        let is_branch = self
            .repository
            .0
            .resolve_reference_from_short_name(name)
            .map(|reference| reference.is_branch() || reference.is_remote())?;

        if !is_branch {
            return Err(Error::NotBranch(branch_name));
        }

        let branch = self.get_history(name.to_string())?;
        self.set(branch);
        Ok(())
    }

    /// Set the current `Browser`'s [`History`] to the [`TagName`] provided.
    ///
    /// # Errors
    ///
    /// * [`error::Error::Git`]
    /// * [`error::Error::NotTag`]
    ///
    /// # Examples
    ///
    /// ```
    /// use nonempty::NonEmpty;
    /// use radicle_surf::vcs::History;
    /// use radicle_surf::vcs::git::{TagName, Browser, Oid, Repository};
    ///
    /// let repo = Repository::new("./data/git-platinum").unwrap();
    /// let mut browser = Browser::new(repo).unwrap();
    ///
    /// // Switch to "v0.3.0"
    /// browser.tag(TagName::new("v0.3.0")).expect("Failed to switch tag");
    ///
    /// let expected_history = History(NonEmpty::from((
    ///     Oid::from_str("19bec071db6474af89c866a1bd0e4b1ff76e2b97").unwrap(),
    ///     vec![
    ///         Oid::from_str("f3a089488f4cfd1a240a9c01b3fcc4c34a4e97b2").unwrap(),
    ///         Oid::from_str("2429f097664f9af0c5b7b389ab998b2199ffa977").unwrap(),
    ///         Oid::from_str("d3464e33d75c75c99bfb90fa2e9d16efc0b7d0e3").unwrap(),
    ///     ]
    /// )));
    ///
    /// let history_ids = browser.get().map(|commit| commit.id);
    ///
    /// // We are able to render the directory
    /// assert_eq!(history_ids, expected_history);
    /// ```
    pub fn tag(&mut self, tag_name: TagName) -> Result<(), Error> {
        let name = tag_name.name();

        if !self
            .repository
            .0
            .resolve_reference_from_short_name(name)?
            .is_tag()
        {
            return Err(Error::NotTag(tag_name));
        }

        let tag = self.get_history(name.to_string())?;
        self.set(tag);
        Ok(())
    }

    /// Set the current `Browser`'s [`History`] to the [`Oid`] (SHA digest) provided.
    ///
    /// # Errors
    ///
    /// * [`error::Error::Git`]
    ///
    /// # Examples
    ///
    /// ```
    /// use radicle_surf::file_system::{Label, SystemType};
    /// use radicle_surf::file_system::unsound;
    /// use radicle_surf::vcs::git::{Browser, Oid, Repository};
    /// use std::str::FromStr;
    ///
    /// let repo = Repository::new("./data/git-platinum")
    ///     .expect("Could not retrieve ./data/git-platinum as git repository");
    /// let mut browser = Browser::new(repo).expect("Could not initialise Browser");
    ///
    /// let commit = Oid::from_str(
    ///     "e24124b7538658220b5aaf3b6ef53758f0a106dc").expect("Failed to
    /// parse SHA");
    /// // Set to the initial commit
    /// let commit = Oid::from_str(
    ///     "e24124b7538658220b5aaf3b6ef53758f0a106dc"
    /// ).expect("Failed to parse SHA");
    ///
    /// browser.commit(commit).expect("Missing commit");
    ///
    /// let directory = browser.get_directory().unwrap();
    /// let mut directory_contents = directory.list_directory();
    ///
    /// assert_eq!(
    ///     directory_contents,
    ///     vec![
    ///         SystemType::file(unsound::label::new("README.md")),
    ///         SystemType::directory(unsound::label::new("bin")),
    ///         SystemType::directory(unsound::label::new("src")),
    ///         SystemType::directory(unsound::label::new("this")),
    ///     ]
    /// );
    /// ```
    pub fn commit(&mut self, oid: Oid) -> Result<(), Error> {
        let commit = self.repository.get_commit(oid)?;
        let history = self.repository.commit_to_history(commit)?;
        self.set(history);
        Ok(())
    }

    /// Set a `Browser`'s [`History`] based on a [revspec](https://git-scm.com/docs/git-rev-parse.html#_specifying_revisions).
    ///
    /// # Errors
    ///
    /// * [`error::Error::Git`]
    /// * [`error::Error::RevParseFailure`]
    ///
    /// # Examples
    ///
    /// ```
    /// use radicle_surf::file_system::{Label, SystemType};
    /// use radicle_surf::file_system::unsound;
    /// use radicle_surf::vcs::git::{Browser, Oid, Repository};
    /// use std::str::FromStr;
    ///
    /// let repo = Repository::new("./data/git-platinum")
    ///     .expect("Could not retrieve ./data/git-platinum as git repository");
    /// let mut browser = Browser::new(repo).expect("Could not initialise Browser");
    ///
    /// browser
    ///     .revspec("refs/remotes/origin/dev")
    ///     .expect("Missing dev");
    ///
    /// let directory = browser.get_directory().unwrap();
    /// let mut directory_contents = directory.list_directory();
    /// directory_contents.sort();
    ///
    /// assert!(directory_contents.contains(
    ///     &SystemType::file(unsound::label::new("here-we-are-on-a-dev-branch.lol"))
    /// ));
    /// ```
    pub fn revspec(&mut self, spec: &str) -> Result<(), Error> {
        let history = self.get_history(spec.to_string())?;
        self.set(history);
        Ok(())
    }

    /// Set a `Browser`'s `History` based on a [`RevObject`].
    ///
    /// # Errors
    ///
    /// * [`error::Error::Git`]
    ///
    /// This is useful if you already have a [`RevObject`], but
    /// [`revspec`](#method.revspec) would be a more common function to use.
    pub fn rev(&mut self, rev: RevObject) -> Result<(), Error> {
        let repository = &self.repository;
        let commit = rev.into_commit(&repository.0)?;
        let history = repository.commit_to_history(commit)?;
        self.set(history);
        Ok(())
    }

    /// List the names of the _branches_ that are contained in the underlying [`Repository`].
    ///
    /// # Errors
    ///
    /// * [`error::Error::Git`]
    ///
    /// # Examples
    ///
    /// ```
    /// use radicle_surf::vcs::git::{Branch, BranchType, BranchName, Browser, Repository};
    ///
    /// let repo = Repository::new("./data/git-platinum").unwrap();
    /// let mut browser = Browser::new(repo).unwrap();
    ///
    /// let branches = browser.list_branches(None).unwrap();
    ///
    /// // 'master' exists in the list of branches
    /// assert!(branches.contains(&Branch::local(BranchName::new("master"))));
    ///
    /// // Filter the branches by `Remote`.
    /// let mut branches = browser.list_branches(Some(BranchType::Remote)).unwrap();
    /// branches.sort();
    ///
    /// assert_eq!(branches, vec![
    ///     Branch::remote(BranchName::new("origin/HEAD")),
    ///     Branch::remote(BranchName::new("origin/dev")),
    ///     Branch::remote(BranchName::new("origin/master")),
    /// ]);
    /// ```
    pub fn list_branches(&self, filter: Option<BranchType>) -> Result<Vec<Branch>, Error> {
        self.repository.list_branches(filter)
    }

    /// List the names of the _tags_ that are contained in the underlying [`Repository`].
    ///
    /// # Errors
    ///
    /// * [`error::Error::Git`]
    ///
    /// # Examples
    ///
    /// ```
    /// use radicle_surf::vcs::git::{Browser, Repository, TagName};
    ///
    /// let repo = Repository::new("./data/git-platinum").unwrap();
    /// let mut browser = Browser::new(repo).unwrap();
    ///
    /// let tags = browser.list_tags().unwrap();
    ///
    /// assert_eq!(
    ///     tags,
    ///     vec![
    ///         TagName::new("v0.1.0"),
    ///         TagName::new("v0.2.0"),
    ///         TagName::new("v0.3.0"),
    ///         TagName::new("v0.4.0"),
    ///         TagName::new("v0.5.0")
    ///     ]
    /// );
    /// ```
    pub fn list_tags(&self) -> Result<Vec<TagName>, Error> {
        self.repository.list_tags()
    }

    /// Given a [`crate::file_system::Path`] to a file, return the last [`Commit`] that touched that
    /// file or directory.
    ///
    /// # Errors
    ///
    /// * [`error::Error::Git`]
    /// * [`error::Error::LastCommitException`]
    ///
    /// # Examples
    ///
    /// ```
    /// use radicle_surf::vcs::git::{Browser, Oid, Repository};
    /// use radicle_surf::file_system::{Label, Path, SystemType};
    /// use radicle_surf::file_system::unsound;
    /// use std::str::FromStr;
    ///
    /// let repo = Repository::new("./data/git-platinum")
    ///     .expect("Could not retrieve ./data/git-test as git repository");
    /// let mut browser = Browser::new(repo).expect("Could not initialise Browser");
    ///
    /// // Clamp the Browser to a particular commit
    /// let commit = Oid::from_str(
    ///     "d6880352fc7fda8f521ae9b7357668b17bb5bad5"
    /// ).expect("Failed to parse SHA");
    /// browser.commit(commit).expect("Failed to set commit");
    ///
    /// let head_commit = browser.get().first().clone();
    /// let expected_commit = Oid::from_str("d3464e33d75c75c99bfb90fa2e9d16efc0b7d0e3")
    ///     .expect("Failed to create Oid");
    ///
    /// let readme_last_commit = browser
    ///     .last_commit(&Path::with_root(&[unsound::label::new("README.md")]))
    ///     .expect("Failed to get last commit")
    ///     .map(|commit| commit.id);
    ///
    /// assert_eq!(readme_last_commit, Some(expected_commit));
    ///
    /// let expected_commit = Oid::from_str("e24124b7538658220b5aaf3b6ef53758f0a106dc")
    ///     .expect("Failed to create Oid");
    ///
    /// let memory_last_commit = browser
    ///     .last_commit(&Path::with_root(&[unsound::label::new("src"), unsound::label::new("memory.rs")]))
    ///     .expect("Failed to get last commit")
    ///     .map(|commit| commit.id);
    ///
    /// assert_eq!(memory_last_commit, Some(expected_commit));
    /// ```
    pub fn last_commit(&self, path: &file_system::Path) -> Result<Option<Commit>, Error> {
        let file_history = self.repository.file_history(self.get().first().clone())?;

        Ok(file_history.find(&path.0).map(|tree| {
            tree.maximum_by(&|c: &NonEmpty<OrderedCommit>, d| c.first().compare_by_id(&d.first()))
                .first()
                .commit
                .clone()
        }))
    }

    /// Do a pre-order TreeWalk of the given commit. This turns a Tree
    /// into a HashMap of Paths and a list of Files. We can then turn that
    /// into a Directory.
    fn get_tree(
        repo: &git2::Repository,
        commit: &Commit,
    ) -> Result<HashMap<file_system::Path, NonEmpty<(file_system::Label, directory::File)>>, Error>
    {
        let mut file_paths_or_error: Result<
            HashMap<file_system::Path, NonEmpty<(file_system::Label, directory::File)>>,
            Error,
        > = Ok(HashMap::new());

        let commit = repo.find_commit(commit.id)?;
        let tree = commit.as_object().peel_to_tree()?;

        tree.walk(
            git2::TreeWalkMode::PreOrder,
            |s, entry| match Self::tree_entry_to_file_and_path(repo, s, entry) {
                Ok((path, name, file)) => {
                    match file_paths_or_error.as_mut() {
                        Ok(mut files) => Self::update_file_map(path, name, file, &mut files),

                        // We don't need to update, we want to keep the error.
                        Err(_err) => {},
                    }
                    git2::TreeWalkResult::Ok
                },
                Err(err) => match err {
                    // We want to continue if the entry was not a Blob.
                    TreeWalkError::NotBlob => git2::TreeWalkResult::Ok,

                    // We found a ObjectType::Commit (likely a submodule) and
                    // so we can skip it.
                    TreeWalkError::Commit => git2::TreeWalkResult::Ok,

                    // But we want to keep the error and abort otherwise.
                    TreeWalkError::Git(err) => {
                        file_paths_or_error = Err(err);
                        git2::TreeWalkResult::Abort
                    },
                },
            },
        )?;

        file_paths_or_error
    }

    fn update_file_map(
        path: file_system::Path,
        name: file_system::Label,
        file: directory::File,
        files: &mut HashMap<file_system::Path, NonEmpty<(file_system::Label, directory::File)>>,
    ) {
        files
            .entry(path)
            .and_modify(|entries| entries.push((name.clone(), file.clone())))
            .or_insert_with(|| NonEmpty::new((name, file)));
    }

    fn tree_entry_to_file_and_path(
        repo: &git2::Repository,
        tree_path: &str,
        entry: &git2::TreeEntry,
    ) -> Result<(file_system::Path, file_system::Label, directory::File), TreeWalkError> {
        // Account for the "root" of git being the empty string
        let path = if tree_path.is_empty() {
            Ok(file_system::Path::root())
        } else {
            file_system::Path::try_from(tree_path)
        }?;

        // We found a Commit object in the Tree, likely a submodule.
        // We will skip this entry.
        if let Some(git2::ObjectType::Commit) = entry.kind() {
            return Err(TreeWalkError::Commit);
        }

        let object = entry.to_object(repo)?;
        let blob = object.as_blob().ok_or(TreeWalkError::NotBlob)?;
        let name = str::from_utf8(entry.name_bytes())?;

        let name = file_system::Label::try_from(name).map_err(Error::FileSystem)?;

        Ok((
            path,
            name,
            directory::File {
                contents: blob.content().to_owned(),
                size: blob.size(),
            },
        ))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    // An issue with submodules, see: https://github.com/radicle-dev/radicle-surf/issues/54
    fn test_submodule_failure() {
        let repo = Repository::new(".").unwrap();
        let browser = Browser::new(repo).unwrap();

        browser.get_directory().unwrap();
    }

    #[cfg(test)]
    mod rev {
        use super::{Browser, Error, Oid, Repository};

        // **FIXME**: This seems to break occasionally on
        // buildkite. For some reason the commit 3873745c8f6ffb45c990eb23b491d4b4b6182f95,
        // which is on master (currently HEAD), is not found. It seems to load the history
        // with d6880352fc7fda8f521ae9b7357668b17bb5bad5 as the HEAD.
        //
        // To temporarily fix this, we need to select "New Build" from the build kite build page
        // that's failing.
        // * Under "Message" put whatever you want.
        // * Under "Branch" put in the branch you're working on.
        // * Expand "Options" and select "clean checkout".
        #[test]
        fn _master() -> Result<(), Error> {
            let repo = Repository::new("./data/git-platinum")?;
            let mut browser = Browser::new(repo)?;
            browser.revspec("master")?;

            let commit1 = Oid::from_str("3873745c8f6ffb45c990eb23b491d4b4b6182f95")?;
            assert!(
                browser
                    .history
                    .find(|commit| if commit.id == commit1 {
                        Some(commit.clone())
                    } else {
                        None
                    })
                    .is_some(),
                "commit_id={}, history =\n{:#?}",
                commit1,
                browser.history
            );

            let commit2 = Oid::from_str("d6880352fc7fda8f521ae9b7357668b17bb5bad5")?;
            assert!(
                browser
                    .history
                    .find(|commit| if commit.id == commit2 {
                        Some(commit.clone())
                    } else {
                        None
                    })
                    .is_some(),
                "commit_id={}, history =\n{:#?}",
                commit2,
                browser.history
            );

            Ok(())
        }

        #[test]
        fn commit() -> Result<(), Error> {
            let repo = Repository::new("./data/git-platinum")?;
            let mut browser = Browser::new(repo)?;
            browser.revspec("3873745c8f6ffb45c990eb23b491d4b4b6182f95")?;

            let commit1 = Oid::from_str("3873745c8f6ffb45c990eb23b491d4b4b6182f95")?;
            assert!(browser
                .history
                .find(|commit| if commit.id == commit1 {
                    Some(commit.clone())
                } else {
                    None
                })
                .is_some());

            Ok(())
        }

        #[test]
        fn commit_short() -> Result<(), Error> {
            let repo = Repository::new("./data/git-platinum")?;
            let mut browser = Browser::new(repo)?;
            browser.revspec("3873745c8")?;

            let commit1 = Oid::from_str("3873745c8f6ffb45c990eb23b491d4b4b6182f95")?;
            assert!(browser
                .history
                .find(|commit| if commit.id == commit1 {
                    Some(commit.clone())
                } else {
                    None
                })
                .is_some());

            Ok(())
        }

        #[test]
        fn tag() -> Result<(), Error> {
            let repo = Repository::new("./data/git-platinum")?;
            let mut browser = Browser::new(repo)?;
            browser.revspec("v0.2.0")?;

            let commit1 = Oid::from_str("2429f097664f9af0c5b7b389ab998b2199ffa977")?;
            assert_eq!(browser.history.first().id, commit1);

            Ok(())
        }
    }

    #[cfg(test)]
    mod last_commit {
        use crate::file_system::unsound;
        use crate::file_system::Path;
        use crate::vcs::git::{Browser, Oid, Repository};

        #[test]
        fn readme_missing_and_memory() {
            let repo = Repository::new("./data/git-platinum")
                .expect("Could not retrieve ./data/git-platinum as git repository");
            let mut browser = Browser::new(repo).expect("Could not initialise Browser");

            // Set the browser history to the initial commit
            let commit = Oid::from_str("d3464e33d75c75c99bfb90fa2e9d16efc0b7d0e3")
                .expect("Failed to parse SHA");
            browser.commit(commit).unwrap();

            let head_commit = browser.get().0.first().clone();

            // memory.rs is commited later so it should not exist here.
            let memory_last_commit = browser
                .last_commit(&Path::with_root(&[
                    unsound::label::new("src"),
                    unsound::label::new("memory.rs"),
                ]))
                .expect("Failed to get last commit")
                .map(|commit| commit.id);

            assert_eq!(memory_last_commit, None);

            // README.md exists in this commit.
            let readme_last_commit = browser
                .last_commit(&Path::with_root(&[unsound::label::new("README.md")]))
                .expect("Failed to get last commit")
                .map(|commit| commit.id);

            assert_eq!(readme_last_commit, Some(head_commit.id));
        }

        #[test]
        fn folder_svelte() {
            let repo = Repository::new("./data/git-platinum")
                .expect("Could not retrieve ./data/git-platinum as git repository");
            let mut browser = Browser::new(repo).expect("Could not initialise Browser");

            // Check that last commit is the actual last commit even if head commit differs.
            let commit = Oid::from_str("19bec071db6474af89c866a1bd0e4b1ff76e2b97")
                .expect("Could not parse SHA");
            browser.commit(commit).unwrap();

            let expected_commit_id =
                Oid::from_str("f3a089488f4cfd1a240a9c01b3fcc4c34a4e97b2").unwrap();

            let folder_svelte = browser
                .last_commit(&unsound::path::new("~/examples/Folder.svelte"))
                .expect("Failed to get last commit")
                .map(|commit| commit.id);

            assert_eq!(folder_svelte, Some(expected_commit_id));
        }

        #[test]
        fn nest_directory() {
            let repo = Repository::new("./data/git-platinum")
                .expect("Could not retrieve ./data/git-platinum as git repository");
            let mut browser = Browser::new(repo).expect("Could not initialise Browser");

            // Check that last commit is the actual last commit even if head commit differs.
            let commit = Oid::from_str("19bec071db6474af89c866a1bd0e4b1ff76e2b97")
                .expect("Failed to parse SHA");
            browser.commit(commit).unwrap();

            let expected_commit_id =
                Oid::from_str("2429f097664f9af0c5b7b389ab998b2199ffa977").unwrap();

            let nested_directory_tree_commit_id = browser
                .last_commit(&unsound::path::new(
                    "~/this/is/a/really/deeply/nested/directory/tree",
                ))
                .expect("Failed to get last commit")
                .map(|commit| commit.id);

            assert_eq!(nested_directory_tree_commit_id, Some(expected_commit_id));
        }

        #[test]
        fn root() {
            let repo = Repository::new("./data/git-platinum")
                .expect("Could not retrieve ./data/git-platinum as git repository");
            let browser = Browser::new(repo).expect("Could not initialise Browser");

            let expected_commit_id =
                Oid::from_str("3873745c8f6ffb45c990eb23b491d4b4b6182f95").unwrap();

            let root_last_commit_id = browser
                .last_commit(&Path::root())
                .expect("Failed to get last commit")
                .map(|commit| commit.id);

            assert_eq!(root_last_commit_id, Some(expected_commit_id));
        }
    }
}
