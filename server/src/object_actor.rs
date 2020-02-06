use crate::chat::ServerMessage;
use crate::lua::LuaHost;
use crate::lua::SerializableValue;
use crate::world::*;
use actix::{Actor, Context, Handler, Message};
use rlua;
use serde::{Deserialize, Serialize};

pub struct ObjectActor {
  id: Id,
  world: WorldRef,
  lua_state: rlua::Lua,
  state: ObjectActorState,
}

#[derive(Serialize, Deserialize, Clone)]
pub struct ObjectActorState {}

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
      state: state.unwrap_or(ObjectActorState {}),
      lua_state: lua_host.fresh_state().unwrap(),
    }
  }
}

impl Actor for ObjectActor {
  type Context = Context<Self>;

  fn started(&mut self, ctx: &mut Self::Context) {
    self
      .lua_state
      .context::<_, rlua::Result<()>>(|lua_ctx| {
        let globals = lua_ctx.globals();

        let orisa = lua_ctx.create_table()?;
        let wf = self.world.clone();
        orisa.set("id", self.id)?;
        orisa.set(
          "get_children",
          lua_ctx.create_function(move |lua_ctx, object_id: Id| {
            Ok(wf.read(|w| w.children(object_id).collect::<Vec<Id>>()))
          })?,
        )?;

        let wf = self.world.clone();
        let id = self.id;
        orisa.set(
          "send",
          lua_ctx.create_function(
            move |lua_ctx, (object_id, name, payload): (Id, String, SerializableValue)| {
              Ok(wf.read(|w| {
                w.send_message(
                  object_id,
                  ObjectMessage {
                    immediate_sender: id,
                    name: name,
                    payload: payload,
                  },
                )
              }))
            },
          )?,
        )?;

        let wf = self.world.clone();
        orisa.set(
          "tell",
          lua_ctx.create_function(move |lua_ctx, (message): (String)| {
            Ok(wf.read(|w| w.send_client_message(id, ServerMessage::new(&message))))
          })?,
        )?;

        let wf = self.world.clone();
        orisa.set(
          "name",
          lua_ctx.create_function(move |lua_ctx, (id): (Id)| Ok(wf.read(|w| w.username(id))))?,
        )?;

        globals.set("orisa", orisa)?;
        Ok(())
      })
      .unwrap();
  }
}

impl Handler<ObjectMessage> for ObjectActor {
  type Result = ();

  fn handle(&mut self, msg: ObjectMessage, _ctx: &mut Self::Context) {
    let sender = msg.immediate_sender;
    // we hold a read lock on the world as a simple form of "transaction isolation" for now
    // this is not useful right now but prevents us from accidentally writing to the world
    // which could produce globally-visible effects while other objects are running.
    self.world.read(|w| {
      let kind = w.kind(self.id);
      let _ = self
        .lua_state
        .context(|lua_ctx| {
          let globals = lua_ctx.globals();
          let orisa: rlua::Table = globals.get("orisa")?;
          orisa.set("kind", kind.0)?;
          orisa.set("sender", sender)?;

          let main: rlua::Function = globals.get("main")?;
          main.call((msg.name, msg.payload))?;
          Ok(())
        })
        .map_err(|err: rlua::Error| log::error!("Failed running payload: {:?}", err));
    });
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
