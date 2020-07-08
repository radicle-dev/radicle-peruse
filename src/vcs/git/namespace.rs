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

pub use git2::Oid;
use std::{convert::TryFrom, fmt, str};

/// A `Namespace` value allows us to switch the git namespace of
/// [`super::Browser`].
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct Namespace {
    /// Since namespaces can be nested we have a vector of strings.
    /// This means that the namespaces `"foo/bar"` is represented as
    /// `vec!["foo", "bar"]`.
    pub(super) values: Vec<String>,
}

impl fmt::Display for Namespace {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.values.join("/"))
    }
}

impl From<&str> for Namespace {
    fn from(namespace: &str) -> Namespace {
        let values = namespace.split('/').map(|n| n.to_string()).collect();
        Self { values }
    }
}

impl TryFrom<&[u8]> for Namespace {
    type Error = str::Utf8Error;

    fn try_from(namespace: &[u8]) -> Result<Self, Self::Error> {
        str::from_utf8(namespace).map(Namespace::from)
    }
}

impl TryFrom<git2::Reference<'_>> for Namespace {
    type Error = str::Utf8Error;

    fn try_from(reference: git2::Reference) -> Result<Self, Self::Error> {
        let re = regex::Regex::new(r"refs/namespaces/([^/]+)/").unwrap();
        let ref_name = str::from_utf8(reference.name_bytes())?;
        let values = re
            .find_iter(ref_name)
            .map(|m| {
                String::from(
                    m.as_str()
                        .trim_start_matches("refs/namespaces/")
                        .trim_end_matches('/'),
                )
            })
            .collect::<Vec<_>>()
            .to_vec();

        Ok(Namespace { values })
    }
}
