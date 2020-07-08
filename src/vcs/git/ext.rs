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

/// Try to strip any refs/namespaces, refs/heads, refs/remotes, and
/// refs/tags. If this fails we return the original string.
pub fn try_extract_refname(spec: &str) -> Result<&str, &str> {
    let re = regex::Regex::new(r"(refs/namespaces/.*)*(refs/heads/|refs/remotes/|refs/tags/)(.*)")
        .unwrap();

    re.captures(spec)
        .and_then(|c| c.get(3).map(|m| m.as_str()))
        .ok_or(spec)
}

/// [`git2::Reference::is_tag`] just does a check for the prefix of `tags/`.
/// This issue with that is, as soon as we're in 'namespaces' ref that
/// is a tag it will say that it's not a tag. Instead we do a regex check on
/// `refs/tags/.*`.
pub fn is_tag(reference: &git2::Reference) -> bool {
    let re = regex::Regex::new(r"refs/tags/.*").unwrap();
    // If we couldn't parse the name we say it's not a tag.
    match reference.name() {
        Some(name) => re.is_match(name),
        None => false,
    }
}

pub fn is_branch(reference: &git2::Reference) -> bool {
    let re = regex::Regex::new(r"refs/heads/.*|refs/remotes/.*/.*").unwrap();
    // If we couldn't parse the name we say it's not a branch.
    match reference.name() {
        Some(name) => re.is_match(name),
        None => false,
    }
}
