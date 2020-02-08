use crate::lua::LuaHost;
use crate::lua::SerializableValue;
use crate::object;
use crate::world::*;

use actix::{Actor, Context, Handler, Message};
use rlua;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

pub struct ObjectActor {
  pub(super) id: Id,
  pub(super) world: WorldRef,
  pub(super) lua_state: rlua::Lua,
  pub(super) state: ObjectActorState,
}

#[derive(Serialize, Deserialize, Clone)]
pub struct ObjectActorState {
  pub(super) persistent_state: HashMap<String, SerializableValue>,
}

impl ObjectActor {
  pub fn new(
    id: Id,
    world_ref: WorldRef,
    state: Option<ObjectActorState>,
    lua_host: &LuaHost,
  ) -> ObjectActor {
    ObjectActor {
      id: id,
      world: world_ref,
      state: state.unwrap_or(ObjectActorState {
        persistent_state: HashMap::new(),
      }),
      lua_state: lua_host.fresh_state().unwrap(),
    }
  }

  fn run_main(&mut self, msg: ObjectMessage) -> rlua::Result<()> {
    // we hold a read lock on the world as a simple form of "transaction isolation" for now
    // this is not useful right now but prevents us from accidentally writing to the world
    // which could produce globally-visible effects while other objects are running.
    let wf = self.world.clone();
    let id = self.id;
    wf.read(|_w| {
      object::api::with_api(self, |lua_ctx| {
        let globals = lua_ctx.globals();
        let orisa: rlua::Table = globals.get("orisa")?;
        orisa.set("self", id)?;
        let main: rlua::Function = globals.get("main")?;

        main.call::<_, ()>((msg.immediate_sender, msg.name, msg.payload))
      })
    })
  }
}

impl Actor for ObjectActor {
  type Context = Context<Self>;

  fn started(&mut self, _ctx: &mut Self::Context) {
    self
      .lua_state
      .context(|ctx| object::api::register_api(ctx))
      .unwrap();
  }
}

impl Handler<ObjectMessage> for ObjectActor {
  type Result = ();

  fn handle(&mut self, msg: ObjectMessage, _ctx: &mut Self::Context) {
    let _ = self
      .run_main(msg)
      .map_err(|err: rlua::Error| log::error!("Failed running payload: {:?}", err));
  }
}

impl Handler<FreezeMessage> for ObjectActor {
  type Result = Option<FreezeResponse>;

  fn handle(&mut self, _msg: FreezeMessage, _ctx: &mut Self::Context) -> Option<FreezeResponse> {
    Some(FreezeResponse {
      id: self.id,
      state: self.state.clone(),
    })
  }
}

#[derive(Debug)]
pub struct ObjectMessage {
  pub immediate_sender: Id,
  pub name: String,
  pub payload: SerializableValue,
}

impl Message for ObjectMessage {
  type Result = ();
}
