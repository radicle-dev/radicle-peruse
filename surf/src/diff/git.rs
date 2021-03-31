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

use std::convert::TryFrom;

use thiserror::Error;

use crate::{
    diff::{self, Diff, Hunk, Line, LineDiff},
    file_system::Path,
    vcs,
};

/// A Git diff error.
#[derive(Debug, PartialEq, Error)]
#[non_exhaustive]
pub enum Error {
    /// A The path of a file isn't available.
    #[error("couldn't retrieve file path")]
    PathUnavailable,
    /// A patch is unavailable.
    #[error("couldn't retrieve patch for {0}")]
    PatchUnavailable(Path),
    /// A Git delta type isn't currently handled.
    #[error("git delta type is not handled")]
    DeltaUnhandled(git2::Delta),
    /// A Git `DiffLine` is invalid.
    #[error("invalid `git2::DiffLine`")]
    InvalidLineDiff,
}

impl<'a> TryFrom<git2::DiffLine<'a>> for LineDiff {
    type Error = Error;

    fn try_from(line: git2::DiffLine) -> Result<Self, Self::Error> {
        match (line.old_lineno(), line.new_lineno()) {
            (None, Some(n)) => Ok(Self::addition(line.content().to_owned(), n)),
            (Some(n), None) => Ok(Self::deletion(line.content().to_owned(), n)),
            (Some(l), Some(r)) => Ok(Self::context(line.content().to_owned(), l, r)),
            (None, None) => Err(Error::InvalidLineDiff),
        }
    }
}

impl<'a> TryFrom<git2::Diff<'a>> for Diff {
    type Error = vcs::git::error::Error;

    fn try_from(git_diff: git2::Diff) -> Result<Diff, Self::Error> {
        use git2::{Delta, Patch};

        let mut diff = Diff::new();

        for (idx, delta) in git_diff.deltas().enumerate() {
            match delta.status() {
                Delta::Added => {
                    let diff_file = delta.new_file();
                    let path = diff_file.path().ok_or(diff::git::Error::PathUnavailable)?;
                    let path = Path::try_from(path.to_path_buf())?;

                    diff.add_created_file(path);
                },
                Delta::Deleted => {
                    let diff_file = delta.old_file();
                    let path = diff_file.path().ok_or(diff::git::Error::PathUnavailable)?;
                    let path = Path::try_from(path.to_path_buf())?;

                    diff.add_deleted_file(path);
                },
                Delta::Modified => {
                    let diff_file = delta.new_file();
                    let path = diff_file.path().ok_or(diff::git::Error::PathUnavailable)?;
                    let path = Path::try_from(path.to_path_buf())?;

                    let patch = Patch::from_diff(&git_diff, idx)?;

                    if let Some(patch) = patch {
                        let mut hunks: Vec<Hunk> = Vec::new();

                        for h in 0..patch.num_hunks() {
                            let (hunk, hunk_lines) = patch.hunk(h)?;
                            let header = Line(hunk.header().to_owned());
                            let mut lines: Vec<LineDiff> = Vec::new();

                            for l in 0..hunk_lines {
                                let line = patch.line_in_hunk(h, l)?;
                                let line = LineDiff::try_from(line)?;
                                lines.push(line);
                            }
                            hunks.push(Hunk { header, lines });
                        }
                        diff.add_modified_file(path, hunks);
                    } else if diff_file.is_binary() {
                        diff.add_modified_binary_file(path);
                    } else {
                        return Err(diff::git::Error::PatchUnavailable(path).into());
                    }
                },
                Delta::Renamed => {
                    let old = delta
                        .old_file()
                        .path()
                        .ok_or(diff::git::Error::PathUnavailable)?;
                    let new = delta
                        .new_file()
                        .path()
                        .ok_or(diff::git::Error::PathUnavailable)?;

                    let old_path = Path::try_from(old.to_path_buf())?;
                    let new_path = Path::try_from(new.to_path_buf())?;

                    diff.add_moved_file(old_path, new_path);
                },
                Delta::Copied => {
                    let old = delta
                        .old_file()
                        .path()
                        .ok_or(diff::git::Error::PathUnavailable)?;
                    let new = delta
                        .new_file()
                        .path()
                        .ok_or(diff::git::Error::PathUnavailable)?;

                    let old_path = Path::try_from(old.to_path_buf())?;
                    let new_path = Path::try_from(new.to_path_buf())?;

                    diff.add_copied_file(old_path, new_path);
                },
                status => {
                    return Err(diff::git::Error::DeltaUnhandled(status).into());
                },
            }
        }

        Ok(diff)
    }
}
