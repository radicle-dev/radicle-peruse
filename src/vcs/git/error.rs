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

//! Collection of errors and helper instances that can occur when performing
//! operations from [`crate::vcs::git`].

use crate::{
    diff,
    file_system,
    vcs::git::object::{BranchName, Namespace, TagName},
};
use std::str;
use thiserror::Error;

/// Enumeration of errors that can occur in operations from [`crate::vcs::git`].
#[derive(Debug, PartialEq, Error)]
#[non_exhaustive]
pub enum Error {
    /// The user tried to fetch a branch, but the name provided does not
    /// exist as a branch. This could mean that the branch does not exist
    /// or that a tag or commit was provided by accident.
    #[error("provided branch name does not exist: {0}")]
    NotBranch(BranchName),
    /// We tried to convert a name into its remote and branch name parts.
    #[error("could not parse '{0}' into a remote name and branch name")]
    ParseRemoteBranch(BranchName),
    /// The user tried to fetch a tag, but the name provided does not
    /// exist as a tag. This could mean that the tag does not exist
    /// or that a branch or commit was provided by accident.
    #[error("provided tag name does not exist: {0}")]
    NotTag(TagName),
    /// A `revspec` was provided that could not be parsed into a branch, tag, or
    /// commit object.
    #[error("provided revspec '{rev}' could not be parsed into a git object")]
    RevParseFailure {
        /// The provided revspec that failed to parse.
        rev: String,
    },
    /// A `revspec` was provided that could not be found in the given
    /// `namespace`.
    #[error("provided revspec '{rev}' could not be parsed into a git object in the namespace '{namespace}'")]
    NamespaceRevParseFailure {
        /// The namespace we are in when attempting to fetch the `rev`.
        namespace: Namespace,
        /// The provided revspec that failed to parse.
        rev: String,
    },
    /// A [`str::Utf8Error`] error, which usually occurs when a git object's
    /// name is not in UTF-8 form and parsing of it as such fails.
    #[error(transparent)]
    Utf8Error(#[from] str::Utf8Error),
    /// An error that comes from performing a [`crate::file_system`] operation.
    #[error(transparent)]
    FileSystem(#[from] file_system::Error),
    /// While attempting to calculate a diff for retrieving the
    /// [`crate::vcs::git::Browser.last_commit()`], the file path was returned
    /// as an `Option::None`.
    #[error("last commit has an invalid file path")]
    LastCommitException,
    /// The requested file was not found.
    #[error("path not found for: {0}")]
    PathNotFound(file_system::Path),
    /// An error that comes from performing a *diff* operations.
    #[error(transparent)]
    Diff(#[from] diff::git::Error),
    /// A wrapper around the generic [`git2::Error`].
    #[error(transparent)]
    Git(#[from] git2::Error),
}

/// A private enum that captures a recoverable and
/// non-recoverable error when walking the git tree.
///
/// In the case of `NotBlob` we abort the the computation but do
/// a check for it and recover.
///
/// In the of `Git` we abort both computations.
#[derive(Debug, Error)]
pub(crate) enum TreeWalkError {
    #[error("entry is not a blob")]
    NotBlob,
    #[error("git object is a commit")]
    Commit,
    #[error(transparent)]
    Git(#[from] Error),
}

impl From<git2::Error> for TreeWalkError {
    fn from(err: git2::Error) -> Self {
        TreeWalkError::Git(err.into())
    }
}

impl From<file_system::Error> for TreeWalkError {
    fn from(err: file_system::Error) -> Self {
        err.into()
    }
}

impl From<str::Utf8Error> for TreeWalkError {
    fn from(err: str::Utf8Error) -> Self {
        err.into()
    }
}
