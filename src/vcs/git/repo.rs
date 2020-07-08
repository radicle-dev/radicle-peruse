// This file is part of radicle-surf
// <https://github.com/radicle-dev/radicle-surf>
//
// Copyright (C) 2019-2020 The Radicle Team <dev@radicle.xyz>
//
// This program is free software: you can redistribute it and/or modify
// it under the terms of the GNU General Public License version 3 or
// later as published by the Free Software Foundation.
//
// This program is distributed in the hope that it will be useful,
// but WITHOUT ANY WARRANTY; without even the implied warranty of
// MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE.  See the
// GNU General Public License for more details.
//
// You should have received a copy of the GNU General Public License
// along with this program. If not, see <https://www.gnu.org/licenses/>.

use crate::{
    diff::*,
    file_system,
    vcs,
    vcs::{
        git::{
            error::*,
            reference::{glob::RefGlob, Ref, Rev},
            Branch,
            BranchType,
            Commit,
            Namespace,
            Signature,
            Tag,
        },
        VCS,
    },
};
use git2::Oid;
use nonempty::NonEmpty;
use std::{collections::HashSet, convert::TryFrom, str};

/// This is for flagging to the `file_history` function that it should
/// stop at the first (i.e. Last) commit it finds for a file.
pub(super) enum CommitHistory {
    Full,
    Last,
}

/// A `History` that uses `git2::Commit` as the underlying artifact.
pub type History = vcs::History<Commit>;

/// Wrapper around the `git2`'s `git2::Repository` type.
/// This is to to limit the functionality that we can do
/// on the underlying object.
pub struct Repository(pub(super) git2::Repository);

/// A reference-only `Repository`. This means that we cannot mutate the
/// underlying `Repository`. Not being able to mutate the `Repository` means
/// that the functions defined for `RepositoryRef` should be thread-safe.
///
/// # Construction
///
/// Use the `From<&'a git2::Repository>` implementation to construct a
/// `RepositoryRef`.
pub struct RepositoryRef<'a> {
    pub(super) repo_ref: &'a git2::Repository,
}

// RepositoryRef should be safe to transfer across thread boundaries since it
// only holds a reference to git2::Repository. git2::Repository is also Send
// (see: https://docs.rs/git2/0.13.5/src/git2/repo.rs.html#46)
unsafe impl<'a> Send for RepositoryRef<'a> {}

impl<'a> From<&'a git2::Repository> for RepositoryRef<'a> {
    fn from(repo_ref: &'a git2::Repository) -> Self {
        RepositoryRef { repo_ref }
    }
}

impl<'a> RepositoryRef<'a> {
    /// What is the current namespace we're browsing in.
    pub fn which_namespace(&self) -> Result<Option<Namespace>, Error> {
        Ok(self
            .repo_ref
            .namespace_bytes()
            .map(Namespace::try_from)
            .transpose()?)
    }

    /// List the branches within a repository, filtering out ones that do not
    /// parse correctly.
    ///
    /// # Errors
    ///
    /// * [`Error::Git`]
    pub fn list_branches(&self, filter: Option<BranchType>) -> Result<Vec<Branch>, Error> {
        let ref_glob = filter.map_or(RefGlob::Branch, RefGlob::from);

        ref_glob
            .references(&self)?
            .iter()
            .try_fold(vec![], |mut acc, reference| {
                let branch = Branch::try_from(reference?)?;
                acc.push(branch);
                Ok(acc)
            })
    }

    /// List the tags within a repository, filtering out ones that do not parse
    /// correctly.
    ///
    /// # Errors
    ///
    /// * [`Error::Git`]
    pub fn list_tags(&self) -> Result<Vec<Tag>, Error> {
        RefGlob::Tag
            .references(&self)?
            .iter()
            .try_fold(vec![], |mut acc, reference| {
                let tag = Tag::try_from(reference?)?;
                acc.push(tag);
                Ok(acc)
            })
    }

    /// List the namespaces within a repository, filtering out ones that do not
    /// parse correctly.
    ///
    /// # Errors
    ///
    /// * [`Error::Git`]
    pub fn list_namespaces(&self) -> Result<Vec<Namespace>, Error> {
        let namespaces: Result<HashSet<Namespace>, Error> = RefGlob::Namespace
            .references(&self)?
            .iter()
            .try_fold(HashSet::new(), |mut acc, reference| {
                let namespace = Namespace::try_from(reference?)?;
                acc.insert(namespace);
                Ok(acc)
            });
        Ok(namespaces?.into_iter().collect())
    }

    pub(super) fn reference<R, P>(&self, reference: R, check: P) -> Result<History, Error>
    where
        R: Into<Ref>,
        P: FnOnce(&git2::Reference) -> Option<Error>,
    {
        let reference = match self.which_namespace()? {
            None => reference.into(),
            Some(namespace) => reference.into().namespaced(namespace),
        }
        .find_ref(&self)?;

        if let Some(err) = check(&reference) {
            return Err(err);
        }

        Ok(self.to_history(&reference)?)
    }

    /// Get the [`Diff`] between two commits.
    pub fn diff(&self, from: Oid, to: Oid) -> Result<Diff, Error> {
        self.diff_commits(None, Some(from), to)
            .and_then(Diff::try_from)
    }

    /// Get the [`Diff`] of a commit with no parents.
    pub fn initial_diff(&self, oid: Oid) -> Result<Diff, Error> {
        self.diff_commits(None, None, oid).and_then(Diff::try_from)
    }

    /// Parse an [`Oid`] from the given string.
    pub fn oid(&self, oid: &str) -> Result<Oid, Error> {
        Ok(self.repo_ref.revparse_single(oid)?.id())
    }

    pub(super) fn rev_to_commit(&self, rev: &Rev) -> Result<git2::Commit, Error> {
        match rev {
            Rev::Oid(oid) => Ok(self.repo_ref.find_commit(*oid)?),
            Rev::Ref(reference) => Ok(reference.find_ref(&self)?.peel_to_commit()?),
        }
    }

    pub(super) fn switch_namespace(&self, namespace: &str) -> Result<(), Error> {
        Ok(self.repo_ref.set_namespace(namespace)?)
    }

    /// Get a particular `Commit`.
    pub(super) fn get_commit(&self, oid: Oid) -> Result<git2::Commit<'a>, Error> {
        let commit = self.repo_ref.find_commit(oid)?;
        Ok(commit)
    }

    /// Build a [`History`] using the `head` reference.
    pub(super) fn head(&self) -> Result<History, Error> {
        let head = self.repo_ref.head()?;
        self.to_history(&head)
    }

    /// Turn a [`git2::Reference`] into a [`History`] by completing
    /// a revwalk over the first commit in the reference.
    pub(super) fn to_history(&self, history: &git2::Reference<'a>) -> Result<History, Error> {
        let head = history.peel_to_commit()?;
        self.commit_to_history(head)
    }

    /// Turn a [`git2::Reference`] into a [`History`] by completing
    /// a revwalk over the first commit in the reference.
    pub(super) fn commit_to_history(&self, head: git2::Commit) -> Result<History, Error> {
        let head_id = head.id();
        let mut commits = NonEmpty::new(Commit::try_from(head)?);
        let mut revwalk = self.repo_ref.revwalk()?;

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

            let commit = Commit::try_from(self.repo_ref.find_commit(commit_id)?)?;
            commits.push(commit);
        }

        Ok(vcs::History(commits))
    }

    /// Extract the signature from a commit
    ///
    /// # Arguments
    ///
    /// `commit_oid` - The object ID of the commit
    /// `field` - the name of the header field containing the signature block;
    ///           pass `None` to extract the default 'gpgsig'
    pub(super) fn extract_signature(
        &self,
        commit_oid: &Oid,
        field: Option<&str>,
    ) -> Result<Option<Signature>, Error> {
        // Match is necessary here because according to the documentation for
        // git_commit_extract_signature at
        // https://libgit2.org/libgit2/#HEAD/group/commit/git_commit_extract_signature
        // the return value for a commit without a signature will be GIT_ENOTFOUND
        match self.repo_ref.extract_signature(commit_oid, field) {
            Err(error) => {
                if error.code() == git2::ErrorCode::NotFound {
                    Ok(None)
                } else {
                    Err(error.into())
                }
            },
            Ok(sig) => Ok(Some(Signature::from(sig.0))),
        }
    }

    pub(crate) fn revision_branches(&self, oid: &Oid) -> Result<Vec<Branch>, Error> {
        let references = RefGlob::LocalBranch.references(self)?;

        let mut contained_branches = vec![];

        references.iter().try_for_each(|reference| {
            let reference = reference?;
            self.reachable_from(&reference, &oid).and_then(|contains| {
                if contains {
                    let branch = Branch::try_from(reference)?;
                    contained_branches.push(branch);
                }
                Ok(())
            })
        })?;

        Ok(contained_branches)
    }

    fn reachable_from(&self, reference: &git2::Reference, oid: &Oid) -> Result<bool, Error> {
        let other = reference.peel_to_commit()?.id();
        let is_descendant = self.repo_ref.graph_descendant_of(other, *oid)?;

        Ok(other == *oid || is_descendant)
    }

    /// Get the history of the file system where the head of the [`NonEmpty`] is
    /// the latest commit.
    pub(super) fn file_history(
        &self,
        path: &file_system::Path,
        commit_history: CommitHistory,
        commit: Commit,
    ) -> Result<Vec<Commit>, Error> {
        let mut revwalk = self.repo_ref.revwalk()?;
        let mut commits = vec![];

        // Set the revwalk to the head commit
        revwalk.push(commit.id)?;

        for commit in revwalk {
            let parent_id: Oid = commit?;
            let parent = self.repo_ref.find_commit(parent_id)?;
            let paths = self.diff_commit_and_parents(path, &parent)?;
            if let Some(_path) = paths {
                commits.push(Commit::try_from(parent)?);
                match &commit_history {
                    CommitHistory::Last => break,
                    CommitHistory::Full => {},
                }
            }
        }

        Ok(commits)
    }

    fn diff_commit_and_parents(
        &self,
        path: &file_system::Path,
        commit: &git2::Commit,
    ) -> Result<Option<file_system::Path>, Error> {
        let mut parents = commit.parents();
        let parent = parents.next().map(|c| c.id());

        let diff = self.diff_commits(Some(path), parent, commit.id())?;
        if let Some(_delta) = diff.deltas().next() {
            Ok(Some(path.clone()))
        } else {
            Ok(None)
        }
    }

    fn diff_commits(
        &self,
        path: Option<&file_system::Path>,
        from: Option<Oid>,
        to: Oid,
    ) -> Result<git2::Diff, Error> {
        let new_tree = self.repo_ref.find_commit(to)?.tree()?;
        let old_tree = from.map_or(Ok(None), |oid| {
            self.repo_ref.find_commit(oid)?.tree().map(Some)
        })?;

        let mut opts = git2::DiffOptions::new();
        if let Some(path) = path {
            opts.pathspec(path);
            // We're skipping the binary pass because we won't be inspecting deltas.
            opts.skip_binary_check(true);
        }

        let diff =
            self.repo_ref
                .diff_tree_to_tree(old_tree.as_ref(), Some(&new_tree), Some(&mut opts))?;

        Ok(diff)
    }
}

impl<'a> VCS<Commit, Error> for RepositoryRef<'a> {
    type HistoryId = Rev;
    type ArtefactId = Oid;

    fn get_history(&self, history_id: Self::HistoryId) -> Result<History, Error> {
        match history_id {
            Rev::Ref(reference) => self.reference(reference, |_| None),
            Rev::Oid(oid) => {
                let commit = self.get_commit(oid)?;
                self.commit_to_history(commit)
            },
        }
    }

    fn get_histories(&self) -> Result<Vec<History>, Error> {
        self.repo_ref
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

impl<'a> std::fmt::Debug for RepositoryRef<'a> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, ".git")
    }
}

impl Repository {
    /// Open a git repository given its URI.
    ///
    /// # Errors
    ///
    /// * [`Error::Git`]
    pub fn new(repo_uri: impl AsRef<std::path::Path>) -> Result<Self, Error> {
        git2::Repository::open(repo_uri)
            .map(Repository)
            .map_err(Error::from)
    }

    /// Since our operations are read-only when it comes to surfing a repository
    /// we have a separate struct called [`RepositoryRef`]. This turns an owned
    /// [`Repository`], the one returend by [`Repository::new`], into a
    /// [`RepositoryRef`].
    pub fn as_ref(&'_ self) -> RepositoryRef<'_> {
        RepositoryRef { repo_ref: &self.0 }
    }
}

impl<'a> From<&'a Repository> for RepositoryRef<'a> {
    fn from(repo: &'a Repository) -> Self {
        repo.as_ref()
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

impl std::fmt::Debug for Repository {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, ".git")
    }
}
