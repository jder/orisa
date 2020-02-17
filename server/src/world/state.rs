use crate::lua::{PackageReference, SerializableValue};
use crate::object::types::{Id, ObjectKind};
use core::fmt::Display;
use serde::*;
use std::collections::HashMap;

#[derive(Debug)]
pub enum Error {
  InvalidObjectId(Id),
}

impl std::error::Error for Error {}

impl Display for Error {
  fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::result::Result<(), std::fmt::Error> {
    match self {
      Error::InvalidObjectId(id) => write!(f, "Invalid object Id {}", id),
    }
  }
}

impl From<Error> for rlua::Error {
  fn from(e: Error) -> rlua::Error {
    rlua::Error::external(e)
  }
}

type Result<T> = std::result::Result<T, Error>;

#[derive(Serialize, Deserialize, Clone)]
struct Object {
  parent: Option<Id>,
  kind: ObjectKind,
  attrs: HashMap<String, SerializableValue>,
  state: HashMap<String, SerializableValue>,
}

impl Object {
  fn new(kind: ObjectKind) -> Object {
    Object {
      parent: None,
      kind: kind,
      attrs: HashMap::new(),
      state: HashMap::new(),
    }
  }
}

#[derive(Serialize, Deserialize, Clone)]
pub struct State {
  objects: Vec<Object>,
  entrance: Id,
  users: HashMap<String, Id>,
  live_packages: HashMap<PackageReference, String>, // string is lua code
}

/// Methods for manipulating the state of the world.
/// For now, we are running in a single-threaded manner,
/// but the hope the interface will permit using MVCC someday,
/// possibly backed by rocksdb or postgres or similar.
///
/// We also use mut vs non-mut methods to indicate which can cause
/// side-effects on the world, with the idea that pure functions can
/// accept a non-mut world.
impl State {
  pub fn new() -> State {
    let entrance = Object::new(ObjectKind::for_room());
    State {
      objects: vec![entrance],
      entrance: Id(0),
      users: HashMap::new(),
      live_packages: HashMap::new(),
    }
  }

  pub fn create_object(&mut self, kind: ObjectKind) -> Id {
    let id = Id(self.objects.len());
    self.objects.push(Object::new(kind));
    id
  }

  fn object(&self, id: Id) -> Result<&Object> {
    self
      .objects
      .get(id.0)
      .ok_or_else(|| Error::InvalidObjectId(id))
  }

  fn object_mut(&mut self, id: Id) -> Result<&mut Object> {
    self
      .objects
      .get_mut(id.0)
      .ok_or_else(|| Error::InvalidObjectId(id))
  }

  pub fn entrance(&self) -> Id {
    self.entrance
  }

  pub fn get_or_create_user(&mut self, username: &str) -> Id {
    if let Some(id) = self.users.get(username) {
      *id
    } else {
      let id = self.create_object(ObjectKind::for_user(username));
      let entrance = self.entrance();
      self.object_mut(id).unwrap().parent = Some(entrance);

      self.users.insert(username.to_string(), id);
      id
    }
  }

  // TODO: move to Object?
  pub fn username(&self, id: Id) -> Option<String> {
    for (key, value) in self.users.iter() {
      if *value == id {
        return Some(key.to_string());
      }
    }
    None
  }

  // TODO: move to Object?
  pub fn children(&self, id: Id) -> impl Iterator<Item = Id> + '_ {
    self
      .objects
      .iter()
      .enumerate()
      .filter(move |(_index, o)| o.parent == Some(id))
      .map(|(index, _o)| Id(index))
  }

  // TODO: move to Object?
  pub fn parent(&self, of: Id) -> Result<Option<Id>> {
    self.object(of).map(|o| o.parent)
  }

  pub fn get_live_package_content(&self, package: PackageReference) -> Option<&String> {
    if !package.is_live_package() {
      log::warn!("Ignoring request to get non-live package");
      return None;
    }
    self.live_packages.get(&package)
  }

  pub fn set_live_package_content(&mut self, package: PackageReference, content: String) {
    // TODO: per-user permissions
    if !package.is_live_package() {
      log::warn!("Ignoring request to set non-live package");
      return;
    }
    self.live_packages.insert(package, content);
  }

  pub fn set_attrs(&mut self, id: Id, new_attrs: HashMap<String, SerializableValue>) -> Result<()> {
    self
      .object_mut(id)
      .map(|o| o.attrs.extend(new_attrs.into_iter()))
  }

  pub fn get_attr(&self, id: Id, name: &str) -> Result<Option<SerializableValue>> {
    return self
      .object(id)
      .map(|o| o.attrs.get(name).map(|v| v.clone()));
  }

  pub fn set_state(
    &mut self,
    id: Id,
    key: &str,
    value: SerializableValue,
  ) -> Result<Option<SerializableValue>> {
    self
      .object_mut(id)
      .map(|o| o.state.insert(key.to_string(), value))
  }

  pub fn get_state(&self, id: Id, name: &str) -> Result<Option<SerializableValue>> {
    return self
      .object(id)
      .map(|o| o.state.get(name).map(|v| v.clone()));
  }

  pub fn move_object(&mut self, child: Id, new_parent: Option<Id>) -> Result<()> {
    self
      .object_mut(child)
      .map(|child| child.parent = new_parent)
  }

  pub fn kind(&self, id: Id) -> Result<ObjectKind> {
    Ok(self.object(id)?.kind.clone())
  }
}
