pub mod actor;
pub mod state;
use self::actor::{ControlMessage, WorldActor};
use crate::chat::{ChatSocket, ToClientMessage};
use crate::lua::LuaHost;
use crate::object::types::Message;
pub use crate::object::types::{Id, ObjectKind};
use crate::util::WeakRw;
use actix;
use actix::Actor;
use multimap::MultiMap;
use serde::{Deserialize, Serialize};
use serde_json;
pub use state::State;
use std::io::{Read, Write};
use std::sync::{Arc, RwLock};

pub struct World {
  state: State,
  actor: actix::Addr<WorldActor>,
  lua_host: LuaHost,
  chat_connections: MultiMap<Id, actix::Addr<ChatSocket>>,
}

/// Weak reference to the world for use by ObjectActors
pub type WorldRef = WeakRw<World>;

#[derive(Serialize, Deserialize, Clone)]
struct SaveState {
  state: State,
  // Maybe other things like user accounts, etc
}

impl World {
  pub fn register_chat_connect(&mut self, id: Id, connection: actix::Addr<ChatSocket>) {
    self.chat_connections.insert(id, connection)
  }

  pub fn remove_chat_connection(&mut self, id: Id, connection: actix::Addr<ChatSocket>) {
    if let Some(connections) = self.chat_connections.get_vec_mut(&id) {
      if let Some(pos) = connections.iter().position(|x| *x == connection) {
        connections.remove(pos);
      }
    }
  }

  pub fn get_state_mut(&mut self) -> &mut State {
    &mut self.state
  }

  pub fn get_state(&self) -> &State {
    &self.state
  }

  pub fn reload_code(&mut self) {
    self.actor.do_send(ControlMessage::ReloadCode);
  }

  pub fn send_message(&mut self, message: Message) {
    self.actor.do_send(message);
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

  pub fn get_lua_host(&self) -> &LuaHost {
    &self.lua_host
  }

  pub fn new(
    arbiter: &actix::Arbiter,
    lua_path: &std::path::Path,
    from: Option<impl Read>,
  ) -> Result<(Arc<RwLock<Option<World>>>, WorldRef), serde_json::error::Error> {
    let arc = Arc::new(RwLock::new(None));

    let world_ref = WorldRef::new(&arc);

    let state = match from {
      None => State::new(),
      Some(r) => {
        let state: SaveState = serde_json::from_reader(r)?;
        state.state
      }
    };

    let lua_host = LuaHost::new(lua_path).unwrap();

    let addr = {
      let lua_host = lua_host.clone();
      let world_ref = world_ref.clone();
      WorldActor::start_in_arbiter(arbiter, move |_ctx| WorldActor::new(&lua_host, &world_ref))
    };

    let world = World {
      state: state,
      actor: addr,
      chat_connections: MultiMap::new(),
      lua_host: lua_host,
    };

    {
      let mut maybe_world = arc.write().unwrap();
      *maybe_world = Some(world);
    }
    Ok((arc, world_ref))
  }

  pub fn save(&self, w: impl Write) -> Result<(), serde_json::Error> {
    // TODO: this drops any oustanding (queued in actor) messages.
    let state = SaveState {
      state: self.state.clone(),
    };
    serde_json::to_writer_pretty(w, &state)
  }
}
