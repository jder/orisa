use crate::chat::{ChatSocket, ToClientMessage};
use crate::lua::{LuaHost, SerializableValue};
use crate::object::actor::*;
use crate::object::executor::{ExecutorCache, ObjectExecutor};
pub use crate::object::types::{Id, ObjectKind};
use crate::util::WeakRw;
use actix::{Actor, Addr, Arbiter, Message};
use futures::executor;
use futures::stream::FuturesUnordered;
use futures::stream::StreamExt;
use multimap::MultiMap;
use serde::{Deserialize, Serialize};
use serde_json;
use std::collections::HashMap;
use std::io::{Read, Write};
use std::ops::{Deref, DerefMut};
use std::sync::{Arc, Mutex, RwLock};

#[derive(Serialize, Deserialize, Clone)]
struct Object {
  id: Id,
  parent_id: Option<Id>,
  kind: ObjectKind,
  initialized: bool,
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
  local_packages: HashMap<ObjectKind, String>, // string is lua code
  object_attrs: HashMap<Id, HashMap<String, SerializableValue>>,
}

#[derive(Serialize, Deserialize)]
struct SaveState {
  world_state: WorldState,
  actor_state: HashMap<Id, ObjectActorState>,
}

/// Weak reference to the world for use by ObjectActors
pub type WorldRef = WeakRw<World>;

impl World {
  pub fn create_in(&mut self, parent: Option<Id>, kind: ObjectKind) -> Id {
    let id = self.start_create_object(parent, kind);
    self.finish_create_object(id);
    id
  }

  // Allow two-phase init of objects so we can allocate ids sync.
  // You must call finish_create_object before sending any messages to this object.
  pub fn start_create_object(&mut self, parent: Option<Id>, kind: ObjectKind) -> Id {
    let id = Id::new(self.state.objects.len());

    let o = Object {
      id: id,
      parent_id: parent,
      kind: kind,
      initialized: false,
    };
    self.state.objects.push(o);
    id
  }

  // complete initialization of this object and spawn the associated actor
  pub fn finish_create_object(&mut self, id: Id) {
    let mut o = self.get_mut(id);
    assert!(o.initialized == false);
    o.initialized = true;

    let world_ref = self.own_ref.clone();
    let addr = ObjectActor::start_in_arbiter(&self.arbiter, move |_ctx| {
      ObjectActor::new(id, world_ref, None)
    });
    self.actors.insert(id, addr);
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
      let id = self.create_in(Some(entrance), ObjectKind::user(username));
      self.state.users.insert(username.to_string(), id);
      id
    }
  }

  pub fn username(&self, id: Id) -> Option<String> {
    for (key, value) in self.state.users.iter() {
      if *value == id {
        return Some(key.to_string());
      }
    }
    None
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

  pub fn reload_code(&mut self) {
    log::info!("reloading code");
    // TODO: only reload for a particular kind/space/user, etc
    let mut caches = self.executor_caches.lock().unwrap();
    for (_kind, cache) in caches.iter_mut() {
      cache.update()
    }
    log::info!("finished reload");
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

  fn get_mut(&mut self, id: Id) -> &mut Object {
    self.state.objects.get_mut(id.0).unwrap()
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

    let world_ref = WorldRef::new(&arc);

    let world = World {
      state: WorldState {
        objects: vec![],
        entrance_id: None,
        users: HashMap::new(),
        local_packages: HashMap::new(),
        object_attrs: HashMap::new(),
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

  pub fn get_executor(&mut self, kind: ObjectKind) -> ObjectExecutorGuard {
    let mut caches = self.executor_caches.lock().unwrap();
    let cache = caches.entry(kind.clone()).or_insert(ExecutorCache::new());
    ObjectExecutorGuard {
      executor: Some(cache.checkout_executor(&self.lua_host)),
      kind: kind,
      world_ref: self.own_ref.clone(),
    }
  }

  pub fn check_in_executor(&mut self, kind: ObjectKind, executor: ObjectExecutor) {
    let mut caches = self.executor_caches.lock().unwrap();
    caches.get_mut(&kind).map(|c| c.checkin_executor(executor));
  }

  pub fn get_lua_host(&self) -> &LuaHost {
    &self.lua_host
  }

  pub fn get_local_package_content(&self, kind: ObjectKind) -> Option<&String> {
    if kind.top_level_package() == ObjectKind::system_package() {
      return None;
    }
    self.state.local_packages.get(&kind)
  }

  pub fn set_local_package_content(&mut self, kind: ObjectKind, content: String) {
    // TODO: per-user permissions
    if kind.top_level_package() == ObjectKind::system_package() {
      log::warn!("Ignoring request to set system space");
      return;
    }
    self.state.local_packages.insert(kind, content);
    self.reload_code(); // TODO: restrict to just this kind
  }

  pub fn set_attrs(&mut self, id: Id, new_attrs: HashMap<String, SerializableValue>) {
    let attrs = self.state.object_attrs.entry(id).or_default();
    attrs.extend(new_attrs.into_iter())
  }

  pub fn get_attr(&self, id: Id, name: &str) -> Option<SerializableValue> {
    return self
      .state
      .object_attrs
      .get(&id)
      .and_then(|attrs| attrs.get(name).map(|v| v.clone()));
  }

  pub fn move_object(&mut self, child: Id, new_parent: Option<Id>) {
    self.get_mut(child).parent_id = new_parent
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

pub struct ObjectExecutorGuard {
  executor: Option<ObjectExecutor>,
  world_ref: WorldRef,
  kind: ObjectKind,
}

impl Deref for ObjectExecutorGuard {
  type Target = ObjectExecutor;

  fn deref(&self) -> &ObjectExecutor {
    self.executor.as_ref().unwrap()
  }
}

impl DerefMut for ObjectExecutorGuard {
  fn deref_mut(&mut self) -> &mut ObjectExecutor {
    self.executor.as_mut().unwrap()
  }
}

impl Drop for ObjectExecutorGuard {
  fn drop(&mut self) {
    let wf = self.world_ref.clone();
    wf.write(|_w| _w.check_in_executor(self.kind.clone(), self.executor.take().unwrap()));
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
