use crate::chat::{ChatSocket, ToClientMessage};
use crate::lua::LuaHost;
use crate::object::actor::*;
use crate::object::executor::{ExecutorCache, ObjectExecutor};
use actix::{Actor, Addr, Arbiter, Message};
use futures::executor;
use futures::stream::FuturesUnordered;
use futures::stream::StreamExt;
use multimap::MultiMap;
use serde::{Deserialize, Serialize};
use serde_json;
use std::collections::HashMap;
use std::fmt;
use std::io::{Read, Write};
use std::sync::{Arc, Mutex, RwLock, Weak};

#[derive(Debug, PartialEq, Clone, Copy, Hash, Eq, Deserialize, Serialize)]
pub struct Id(usize);

impl fmt::Display for Id {
  fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
    write!(f, "#{}", self.0)
  }
}

#[derive(Serialize, Deserialize, Clone, Debug, Hash, PartialEq, Eq)]
pub struct ObjectKind(pub String);

impl ObjectKind {
  pub fn new(name: &str) -> ObjectKind {
    ObjectKind(name.to_string())
  }

  fn user() -> ObjectKind {
    ObjectKind::new("system/user")
  }
  fn room() -> ObjectKind {
    ObjectKind::new("system/room")
  }
}

#[derive(Serialize, Deserialize, Clone)]
struct Object {
  id: Id,
  parent_id: Option<Id>,
  kind: ObjectKind,
}

pub struct World {
  state: WorldState,

  arbiter: Arbiter,
  own_ref: WorldRef,

  lua_host: LuaHost,
  executor_caches: Mutex<HashMap<ObjectKind, ExecutorCache>>,

  chat_connections: MultiMap<Id, Addr<ChatSocket>>,

  actors: HashMap<Id, Addr<ObjectActor>>,

  frozen: bool,
}

#[derive(Serialize, Deserialize, Clone)]
pub struct WorldState {
  objects: Vec<Object>,
  entrance_id: Option<Id>, // only None during initialization
  users: HashMap<String, Id>,
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
  pub fn create_in(&mut self, parent: Option<Id>, kind: ObjectKind) -> Id {
    let id = Id(self.state.objects.len());

    let world_ref = self.own_ref.clone();
    let addr = ObjectActor::start_in_arbiter(&self.arbiter, move |_ctx| {
      ObjectActor::new(id, world_ref, None)
    });

    let o = Object {
      id: id,
      parent_id: parent,
      kind: kind,
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

  pub fn kind(&self, id: Id) -> ObjectKind {
    self.get(id).kind.clone()
  }

  pub fn get_or_create_user(&mut self, username: &str) -> Id {
    if let Some(id) = self.state.users.get(username) {
      *id
    } else {
      let entrance = self.entrance();
      let id = self.create_in(Some(entrance), ObjectKind::user());
      self.state.users.insert(username.to_string(), id);
      id
    }
  }

  pub fn username(&self, id: Id) -> String {
    for (key, value) in self.state.users.iter() {
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
    // TODO: if we hit this, we should actually have the objects queue up outgoing messages
    // when the world is frozen, to be replayed when they are re-hydrated. (Also might want
    // them to queue inbound messages in this case as well to freeze more quickly.)
    assert!(
      !self.frozen,
      "you cannot send messages while the world is frozen; see freeze_world below"
    );
    self
      .actors
      .get(&id)
      .expect("Can't find actor for given actor id")
      .do_send(message)
  }

  pub fn reload_code(&mut self) -> std::io::Result<()> {
    // TODO: only reload for a particular kind/space/user, etc
    self.lua_host.reload()?;
    let mut caches = self.executor_caches.lock().unwrap();
    for (_, cache) in caches.iter_mut() {
      cache.update(&self.lua_host)
    }
    Ok(())
  }

  pub fn send_client_message(&self, id: Id, message: ToClientMessage) {
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

  fn create_defaults(&mut self) {
    let entrance = self.create_in(None, ObjectKind::room());
    self.state.entrance_id = Some(entrance)
  }

  pub fn new(
    arbiter: Arbiter,
    lua_path: &std::path::Path,
  ) -> (Arc<RwLock<Option<World>>>, WorldRef) {
    let arc = Arc::new(RwLock::new(None));

    let world_ref = WorldRef {
      world: Arc::downgrade(&arc),
    };

    let world = World {
      state: WorldState {
        objects: vec![],
        entrance_id: None,
        users: HashMap::new(),
      },
      arbiter: arbiter,
      own_ref: world_ref.clone(),
      chat_connections: MultiMap::new(),
      actors: HashMap::new(),
      frozen: true,
      lua_host: LuaHost::new(lua_path).unwrap(),
      executor_caches: Mutex::new(HashMap::new()),
    };

    {
      let mut maybe_world = arc.write().unwrap();
      *maybe_world = Some(world);
    }
    (arc, world_ref)
  }

  pub fn with_executor<T, F>(&self, kind: ObjectKind, body: F) -> rlua::Result<T>
  where
    F: FnOnce(&ObjectExecutor) -> rlua::Result<T>,
  {
    // TODO: this could be per space/user instead of kind
    let executor = {
      let mut caches = self.executor_caches.lock().unwrap();
      let cache = caches
        .entry(kind.clone())
        .or_insert(ExecutorCache::new(&self.lua_host));
      cache.checkout_executor()
    };

    let result = body(&executor);

    let mut caches = self.executor_caches.lock().unwrap();
    caches.get_mut(&kind).map(|c| c.checkin_executor(executor));

    result
  }

  pub fn freeze(world_ref: WorldRef, w: impl Write) -> Result<(), serde_json::Error> {
    let future = world_ref.write(|w| {
      // tells actors to queue future messages
      // do this first so that as we freeze them they can finish their current mailbox
      // and the freeze message below is guaranteed to be the last message.
      // the actors kill themselves at this point, and we then remove them from our list of
      // all actors and grab the final world state (which might be edited while the actors
      // spin down.)
      w.frozen = true;

      w.actors
        .iter()
        .map(|(_id, addr)| addr.send(FreezeMessage {}))
        .collect::<FuturesUnordered<actix::prelude::Request<ObjectActor, FreezeMessage>>>()
        .collect::<Vec<_>>()
    });

    let all_states = executor::block_on(future);

    let state = world_ref.write(|w| {
      w.actors.clear();
      w.state.clone()
    });

    let state = SaveState {
      world_state: state,
      actor_state: all_states
        .iter()
        .map(|resp| {
          let response = resp.as_ref().unwrap().as_ref().unwrap(); // surely there is a better way
          (response.id, response.state.clone())
        })
        .collect(),
    };

    serde_json::to_writer_pretty(w, &state)
  }

  pub fn unfreeze_read(&mut self, r: impl Read) -> Result<(), serde_json::Error> {
    assert!(self.frozen, "can only unfreeze when frozen");

    let state: SaveState = serde_json::from_reader(r)?;
    self.state = state.world_state;

    for obj in self.state.objects.iter() {
      let id = obj.id;
      let world_ref = self.own_ref.clone();
      let object_state = state.actor_state.get(&id).map(|state| state.clone());
      let addr = ObjectActor::start_in_arbiter(&self.arbiter, move |_ctx| {
        ObjectActor::new(id, world_ref, object_state)
      });
      self.actors.insert(id, addr);
    }

    self.frozen = false;
    Ok(())
  }

  pub fn unfreeze_empty(&mut self) {
    assert!(self.frozen, "can only unfreeze when frozen");
    assert!(
      self.state.objects.len() == 0,
      "can only unfreeze_empty an empty world; use unfreeze_read"
    );
    self.frozen = false;
    self.create_defaults();
  }
}

pub struct FreezeMessage {}
pub struct FreezeResponse {
  pub id: Id,
  pub state: ObjectActorState,
}

impl Message for FreezeMessage {
  type Result = Option<FreezeResponse>;
}
