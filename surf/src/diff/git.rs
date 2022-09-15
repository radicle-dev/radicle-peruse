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

use crate::{
    diff::{self, Diff, EofNewLine, Hunk, Hunks, Line, LineDiff},
    file_system::Path,
};

pub mod error {
    use thiserror::Error;

    use crate::file_system::{self, Path};

    #[derive(Debug, Eq, Error, PartialEq)]
    #[non_exhaustive]
    pub enum LineDiff {
        /// A Git `DiffLine` is invalid.
        #[error(
            "invalid `git2::DiffLine` which contains no line numbers for either side of the diff"
        )]
        Invalid,
    }

    #[derive(Debug, Error, PartialEq)]
    #[non_exhaustive]
    pub enum Hunk {
        #[error(transparent)]
        Git(#[from] git2::Error),
        #[error(transparent)]
        Line(#[from] LineDiff),
    }

    /// A Git diff error.
    #[derive(Debug, PartialEq, Error)]
    #[non_exhaustive]
    pub enum Diff {
        /// A Git delta type isn't currently handled.
        #[error("git delta type is not handled")]
        DeltaUnhandled(git2::Delta),
        #[error(transparent)]
        FileSystem(#[from] file_system::Error),
        #[error(transparent)]
        Git(#[from] git2::Error),
        #[error(transparent)]
        Hunk(#[from] Hunk),
        #[error(transparent)]
        Line(#[from] LineDiff),
        /// A patch is unavailable.
        #[error("couldn't retrieve patch for {0}")]
        PatchUnavailable(Path),
        /// A The path of a file isn't available.
        #[error("couldn't retrieve file path")]
        PathUnavailable,
    }
}

impl<'a> TryFrom<git2::DiffLine<'a>> for LineDiff {
    type Error = error::LineDiff;

    fn try_from(line: git2::DiffLine) -> Result<Self, Self::Error> {
        match (line.old_lineno(), line.new_lineno()) {
            (None, Some(n)) => Ok(Self::addition(line.content().to_owned(), n)),
            (Some(n), None) => Ok(Self::deletion(line.content().to_owned(), n)),
            (Some(l), Some(r)) => Ok(Self::context(line.content().to_owned(), l, r)),
            (None, None) => Err(error::LineDiff::Invalid),
        }
    }
}

impl<'a> TryFrom<git2::Diff<'a>> for Diff {
    type Error = error::Diff;

    fn try_from(git_diff: git2::Diff) -> Result<Diff, Self::Error> {
        use git2::{Delta, Patch};

        let mut diff = Diff::new();

        for (idx, delta) in git_diff.deltas().enumerate() {
            match delta.status() {
                Delta::Added => {
                    let diff_file = delta.new_file();
                    let path = diff_file.path().ok_or(error::Diff::PathUnavailable)?;
                    let path = Path::try_from(path.to_path_buf())?;

                    let patch = Patch::from_diff(&git_diff, idx)?;
                    if let Some(patch) = patch {
                        diff.add_created_file(
                            path,
                            diff::FileDiff::Plain {
                                hunks: Hunks::try_from(patch)?,
                            },
                        );
                    } else {
                        diff.add_created_file(
                            path,
                            diff::FileDiff::Plain {
                                hunks: Hunks::default(),
                            },
                        );
                    }
                },
                Delta::Deleted => {
                    let diff_file = delta.old_file();
                    let path = diff_file.path().ok_or(error::Diff::PathUnavailable)?;
                    let path = Path::try_from(path.to_path_buf())?;

                    let patch = Patch::from_diff(&git_diff, idx)?;
                    if let Some(patch) = patch {
                        diff.add_deleted_file(
                            path,
                            diff::FileDiff::Plain {
                                hunks: Hunks::try_from(patch)?,
                            },
                        );
                    } else {
                        diff.add_deleted_file(
                            path,
                            diff::FileDiff::Plain {
                                hunks: Hunks::default(),
                            },
                        );
                    }
                },
                Delta::Modified => {
                    let diff_file = delta.new_file();
                    let path = diff_file.path().ok_or(error::Diff::PathUnavailable)?;
                    let path = Path::try_from(path.to_path_buf())?;

                    let patch = Patch::from_diff(&git_diff, idx)?;

                    if let Some(patch) = patch {
                        let mut hunks: Vec<Hunk> = Vec::new();
                        let mut old_missing_eof = false;
                        let mut new_missing_eof = false;

                        for h in 0..patch.num_hunks() {
                            let (hunk, hunk_lines) = patch.hunk(h)?;
                            let header = Line(hunk.header().to_owned());
                            let mut lines: Vec<LineDiff> = Vec::new();

                            for l in 0..hunk_lines {
                                let line = patch.line_in_hunk(h, l)?;
                                match line.origin_value() {
                                    git2::DiffLineType::ContextEOFNL => {
                                        new_missing_eof = true;
                                        old_missing_eof = true;
                                        continue;
                                    },
                                    git2::DiffLineType::AddEOFNL => {
                                        old_missing_eof = true;
                                        continue;
                                    },
                                    git2::DiffLineType::DeleteEOFNL => {
                                        new_missing_eof = true;
                                        continue;
                                    },
                                    _ => {},
                                }
                                let line = LineDiff::try_from(line)?;
                                lines.push(line);
                            }
                            hunks.push(Hunk { header, lines });
                        }
                        let eof = match (old_missing_eof, new_missing_eof) {
                            (true, true) => Some(EofNewLine::BothMissing),
                            (true, false) => Some(EofNewLine::OldMissing),
                            (false, true) => Some(EofNewLine::NewMissing),
                            (false, false) => None,
                        };
                        diff.add_modified_file(path, hunks, eof);
                    } else if diff_file.is_binary() {
                        diff.add_modified_binary_file(path);
                    } else {
                        return Err(error::Diff::PatchUnavailable(path));
                    }
                },
                Delta::Renamed => {
                    let old = delta
                        .old_file()
                        .path()
                        .ok_or(error::Diff::PathUnavailable)?;
                    let new = delta
                        .new_file()
                        .path()
                        .ok_or(error::Diff::PathUnavailable)?;

                    let old_path = Path::try_from(old.to_path_buf())?;
                    let new_path = Path::try_from(new.to_path_buf())?;

                    diff.add_moved_file(old_path, new_path);
                },
                Delta::Copied => {
                    let old = delta
                        .old_file()
                        .path()
                        .ok_or(error::Diff::PathUnavailable)?;
                    let new = delta
                        .new_file()
                        .path()
                        .ok_or(error::Diff::PathUnavailable)?;

                    let old_path = Path::try_from(old.to_path_buf())?;
                    let new_path = Path::try_from(new.to_path_buf())?;

                    diff.add_copied_file(old_path, new_path);
                },
                status => {
                    return Err(error::Diff::DeltaUnhandled(status));
                },
            }
        }

        Ok(diff)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_both_missing_eof_newline() {
        let buf = r#"
diff --git a/.env b/.env
index f89e4c0..7c56eb7 100644
--- a/.env
+++ b/.env
@@ -1 +1 @@
-hello=123
\ No newline at end of file
+hello=1234
\ No newline at end of file
"#;
        let diff = git2::Diff::from_buffer(buf.as_bytes()).unwrap();
        let diff = Diff::try_from(diff).unwrap();
        assert_eq!(diff.modified[0].eof, Some(EofNewLine::BothMissing));
    }

    #[test]
    fn test_none_missing_eof_newline() {
        let buf = r#"
diff --git a/.env b/.env
index f89e4c0..7c56eb7 100644
--- a/.env
+++ b/.env
@@ -1 +1 @@
-hello=123
+hello=1234
"#;
        let diff = git2::Diff::from_buffer(buf.as_bytes()).unwrap();
        let diff = Diff::try_from(diff).unwrap();
        assert_eq!(diff.modified[0].eof, None);
    }

    // TODO(xphoniex): uncomment once libgit2 has fixed the bug
    //#[test]
    fn test_old_missing_eof_newline() {
        let buf = r#"
diff --git a/.env b/.env
index f89e4c0..7c56eb7 100644
--- a/.env
+++ b/.env
@@ -1 +1 @@
-hello=123
\ No newline at end of file
+hello=1234
"#;
        let diff = git2::Diff::from_buffer(buf.as_bytes()).unwrap();
        let diff = Diff::try_from(diff).unwrap();
        assert_eq!(diff.modified[0].eof, Some(EofNewLine::OldMissing));
    }

    // TODO(xphoniex): uncomment once libgit2 has fixed the bug
    //#[test]
    fn test_new_missing_eof_newline() {
        let buf = r#"
diff --git a/.env b/.env
index f89e4c0..7c56eb7 100644
--- a/.env
+++ b/.env
@@ -1 +1 @@
-hello=123
+hello=1234
\ No newline at end of file
"#;
        let diff = git2::Diff::from_buffer(buf.as_bytes()).unwrap();
        let diff = Diff::try_from(diff).unwrap();
        assert_eq!(diff.modified[0].eof, Some(EofNewLine::NewMissing));
    }
}
