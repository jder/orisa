use crate::chat::{ChatSocket, ServerMessage};
use crate::object_actor::*;
use actix::{Actor, Addr, Arbiter};
use multimap::MultiMap;
use serde::{Deserialize, Serialize};
use serde_json;
use std::collections::HashMap;
use std::fmt;
use std::io::{Read, Write};
use std::sync::{Arc, RwLock, Weak};

#[derive(Debug, PartialEq, Clone, Copy, Hash, Eq, Deserialize, Serialize)]
pub struct Id(usize);

impl fmt::Display for Id {
  fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
    write!(f, "#{}", self.0)
  }
}

#[derive(Serialize, Deserialize, Clone)]
struct Object {
  id: Id,
  parent_id: Option<Id>,
}

pub struct World {
  state: WorldState,

  arbiter: Arbiter,
  own_ref: WorldRef,

  chat_connections: MultiMap<Id, Addr<ChatSocket>>,
  users: HashMap<String, Id>,

  actors: HashMap<Id, Addr<ObjectActor>>,
}

#[derive(Serialize, Deserialize, Clone)]
struct WorldState {
  objects: Vec<Object>,
  entrance_id: Option<Id>, // only None during initialization
}

#[derive(Serialize, Deserialize)]
struct SaveState {
  world_state: WorldState,
  actor_state: HashMap<Id, ObjectActorState>,
}

/// Weak reference to the world for use by ObjectActors
#[derive(Clone)]
pub struct WorldRef {
  world: Weak<RwLock<Option<World>>>, // Only None during initialization
}

impl WorldRef {
  pub fn read<F, T>(&self, f: F) -> T
  where
    F: FnOnce(&World) -> T,
  {
    // This is horribly gross which is why we do it here, once.
    let arc = self.world.upgrade().unwrap();
    let guard = arc.read().unwrap();
    let w = guard.as_ref();
    f(&w.unwrap())
  }

  pub fn write<F, T>(&self, f: F) -> T
  where
    F: FnOnce(&mut World) -> T,
  {
    // This is horribly gross which is why we do it here, once.
    let arc = self.world.upgrade().unwrap();
    let mut guard = arc.write().unwrap();
    let w = guard.as_mut();
    f(&mut w.unwrap())
  }
}

impl World {
  pub fn create_in(&mut self, parent: Option<Id>) -> Id {
    let id = Id(self.state.objects.len());

    let world_ref = self.own_ref.clone();
    let addr =
      ObjectActor::start_in_arbiter(&self.arbiter, move |_ctx| ObjectActor::new(id, world_ref));

    let o = Object {
      id: id,
      parent_id: parent,
    };
    self.actors.insert(id, addr);
    self.state.objects.push(o);
    id
  }

  pub fn register_chat_connect(&mut self, id: Id, connection: Addr<ChatSocket>) {
    self.chat_connections.insert(id, connection)
  }

  pub fn remove_chat_connection(&mut self, id: Id, connection: Addr<ChatSocket>) {
    if let Some(connections) = self.chat_connections.get_vec_mut(&id) {
      if let Some(pos) = connections.iter().position(|x| *x == connection) {
        connections.remove(pos);
      }
    }
  }

  pub fn entrance(&self) -> Id {
    self.state.entrance_id.unwrap()
  }

  pub fn get_or_create_user(&mut self, username: &str) -> Id {
    if let Some(id) = self.users.get(username) {
      *id
    } else {
      let entrance = self.entrance();
      let id = self.create_in(Some(entrance));
      self.users.insert(username.to_string(), id);
      id
    }
  }

  pub fn username(&self, id: Id) -> String {
    for (key, value) in self.users.iter() {
      if *value == id {
        return key.to_string();
      }
    }
    return id.to_string();
  }

  pub fn children(&self, id: Id) -> impl Iterator<Item = Id> + '_ {
    self
      .state
      .objects
      .iter()
      .filter(move |o| o.parent_id == Some(id))
      .map(|o| o.id)
  }

  pub fn parent(&self, of: Id) -> Option<Id> {
    self.get(of).parent_id
  }

  pub fn send_message(&self, id: Id, message: ObjectMessage) {
    self.actors.get(&id).unwrap().do_send(message)
  }

  pub fn send_client_message(&self, id: Id, message: ServerMessage) {
    if let Some(connections) = self.chat_connections.get_vec(&id) {
      for conn in connections.iter() {
        conn.do_send(message.clone());
      }
    } else {
      log::warn!(
        "No chat connection for object {}; dropping message {:?}",
        id,
        message
      );
    }
  }

  fn get(&self, id: Id) -> &Object {
    self.state.objects.get(id.0).unwrap()
  }

  #[allow(dead_code)]
  fn get_mut(&mut self, id: Id) -> &mut Object {
    self.state.objects.get_mut(id.0).unwrap()
  }

  fn create_defaults(&mut self) {
    let entrance = self.create_in(None);
    self.state.entrance_id = Some(entrance)
  }

  pub fn new(arbiter: Arbiter) -> (Arc<RwLock<Option<World>>>, WorldRef) {
    let arc = Arc::new(RwLock::new(None));

    let world_ref = WorldRef {
      world: Arc::downgrade(&arc),
    };

    let mut world = World {
      state: WorldState {
        objects: vec![],
        entrance_id: None,
      },
      arbiter: arbiter,
      own_ref: world_ref.clone(),
      chat_connections: MultiMap::new(),
      users: HashMap::new(),
      actors: HashMap::new(),
    };
    world.create_defaults();

    {
      let mut maybe_world = arc.write().unwrap();
      *maybe_world = Some(world);
    }
    (arc, world_ref)
  }

  pub fn save(&self, w: impl Write) -> Result<(), serde_json::Error> {
    let state = SaveState {
      world_state: self.state.clone(),
      actor_state: HashMap::new(), // TODO
    };

    serde_json::to_writer_pretty(w, &state)
  }

  pub fn load(&mut self, r: impl Read) -> Result<(), serde_json::Error> {
    let state: SaveState = serde_json::from_reader(r)?;
    self.state = state.world_state;
    // TODO actors
    Ok(())
  }
}
