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

use serde::{
    ser::{SerializeStruct as _, Serializer},
    Serialize,
};

pub mod blob;
pub use blob::{blob, Blob, BlobContent};

pub mod tree;
pub use tree::{tree, Tree, TreeEntry};

use crate::commit;

/// Git object types.
///
/// `shafiul.github.io/gitbook/1_the_git_object_model.html`
#[derive(Debug, Eq, Ord, PartialOrd, PartialEq)]
pub enum ObjectType {
    /// References a list of other trees and blobs.
    Tree,
    /// Used to store file data.
    Blob,
}

impl Serialize for ObjectType {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        match self {
            Self::Blob => serializer.serialize_unit_variant("ObjectType", 0, "BLOB"),
            Self::Tree => serializer.serialize_unit_variant("ObjectType", 1, "TREE"),
        }
    }
}

/// Set of extra information we carry for blob and tree objects returned from
/// the API.
pub struct Info {
    /// Name part of an object.
    pub name: String,
    /// The type of the object.
    pub object_type: ObjectType,
    /// The last commmit that touched this object.
    pub last_commit: Option<commit::Header>,
}

impl Serialize for Info {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        let mut state = serializer.serialize_struct("Info", 3)?;
        state.serialize_field("name", &self.name)?;
        state.serialize_field("objectType", &self.object_type)?;
        state.serialize_field("lastCommit", &self.last_commit)?;
        state.end()
    }
}
