use crate::lua::{PackageReference, SerializableValue};
use crate::object::types::*;
use core::fmt::Display;
use serde::*;
use std::collections::HashMap;

#[derive(Debug)]
pub enum Error {
  InvalidObjectId(Id),
  CyclicHierarchy { child: Id, parent: Id },
}

impl std::error::Error for Error {}

impl Display for Error {
  fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::result::Result<(), std::fmt::Error> {
    match self {
      Error::InvalidObjectId(id) => write!(f, "Invalid object Id {}", id),
      Error::CyclicHierarchy { child, parent } => write!(
        f,
        "Moving child {} to parent {} causes a cycle",
        child, parent
      ),
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

  #[serde(default)]
  timers: HashMap<String, Timer>,
}

impl Object {
  fn new(kind: ObjectKind) -> Object {
    Object {
      parent: None,
      kind: kind,
      attrs: HashMap::new(),
      state: HashMap::new(),
      timers: HashMap::new(),
    }
  }
}

#[derive(Serialize, Deserialize, Clone)]
pub struct State {
  objects: Vec<Object>,
  entrance: Id,
  users: HashMap<String, Id>,
  live_packages: HashMap<PackageReference, String>, // string is lua code

  #[serde(default)]
  current_time: GameTime,
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
      current_time: Default::default(),
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

  pub fn get_or_create_user(&mut self, username: &str, user_type: &str) -> Id {
    if let Some(id) = self.users.get(username) {
      *id
    } else {
      let id = self.create_object(ObjectKind::for_user(username, user_type));
      let entrance = self.entrance();
      self.object_mut(id).unwrap().parent = Some(entrance);

      self.users.insert(username.to_string(), id);
      id
    }
  }

  pub fn get_all_users(&self) -> &HashMap<String, Id> {
    &self.users
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

  pub fn set_attr(
    &mut self,
    id: Id,
    key: String,
    value: SerializableValue,
  ) -> Result<Option<SerializableValue>> {
    self.object_mut(id).map(|o| o.attrs.insert(key, value))
  }

  pub fn get_attr(&self, id: Id, name: &str) -> Result<Option<SerializableValue>> {
    return self
      .object(id)
      .map(|o| o.attrs.get(name).map(|v| v.clone()));
  }

  pub fn list_attrs(&self, id: Id) -> Result<impl Iterator<Item = &str>> {
    self.object(id).map(|o| o.attrs.keys().map(|s| s.as_str()))
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

  fn causes_cycle(&self, child: Id, new_parent: Id) -> Result<bool> {
    if child == new_parent {
      Ok(true)
    } else {
      match self.object(new_parent)?.parent {
        None => Ok(false),
        Some(grandparent) => self.causes_cycle(child, grandparent),
      }
    }
  }

  pub fn move_object(&mut self, child: Id, new_parent: Option<Id>) -> Result<()> {
    if let Some(p) = new_parent {
      if self.causes_cycle(child, p)? {
        return Err(Error::CyclicHierarchy { child, parent: p });
      }
    }

    self
      .object_mut(child)
      .map(|child| child.parent = new_parent)
  }

  pub fn kind(&self, id: Id) -> Result<ObjectKind> {
    Ok(self.object(id)?.kind.clone())
  }

  pub fn get_current_time(&self) -> GameTime {
    self.current_time
  }

  pub fn set_current_time(&mut self, time: GameTime) {
    self.current_time = time
  }

  pub fn set_timer(&mut self, id: Id, name: String, timer: Timer) -> Result<()> {
    let o = self.object_mut(id)?;
    o.timers.insert(name, timer);
    Ok(())
  }

  pub fn clear_timer(&mut self, id: Id, name: &str) -> Result<()> {
    let o = self.object_mut(id)?;
    o.timers.remove(name);
    Ok(())
  }

  pub fn extract_ready_timers(&mut self, new_time: GameTime) -> Vec<(Id, Timer)> {
    let current_time = self.current_time;
    self
      .objects
      .iter_mut()
      .enumerate()
      .flat_map(|(id, o)| {
        let (mut ready, not_ready) = o
          .timers
          .drain()
          .partition(|(_k, t)| t.target_time <= new_time && t.target_time > current_time);
        o.timers = not_ready;
        ready
          .drain()
          .map(|(_k, t)| (Id(id), t))
          .collect::<Vec<(Id, Timer)>>()
      })
      .collect()
  }
}
