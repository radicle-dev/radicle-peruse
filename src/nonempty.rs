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

use nonempty::NonEmpty;

pub fn split_last<T>(non_empty: &NonEmpty<T>) -> (Vec<T>, T)
where
    T: Clone + Eq,
{
    let (first, middle, last) = non_empty.split();

    // first == last, so drop first
    if first == last && middle.is_empty() {
        (vec![], last.clone())
    } else {
        // Create the prefix vector
        let mut vec = vec![first.clone()];
        let mut middle = middle.to_vec();
        vec.append(&mut middle);
        (vec, last.clone())
    }
}
