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
        // orisa.set("id", self.id)?;
        let wf = self.world.clone();
        orisa.set("id", self.id)?;
        orisa.set(
          "children",
          lua_ctx.create_function(move |lua_ctx, object_id: Id| {
            log::info!("called children with {}", object_id);
            Ok(wf.read(|w| w.children(object_id).collect::<Vec<Id>>()))
          })?,
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
    match msg.payload {
      ObjectMessagePayload::Say { text } => self.world.read(|w| {
        let name = w.username(sender);
        w.children(self.id).for_each(|child| {
          w.send_message(
            child,
            ObjectMessage {
              immediate_sender: self.id,
              payload: ObjectMessagePayload::Tell {
                text: format!("{}: {}", name, text.clone()),
              },
            },
          )
        })
      }),
      //TODO: only handle tells for object types expected to have chat connections (i.e. people)
      ObjectMessagePayload::Tell { text } => self
        .world
        .read(|w| w.send_client_message(self.id, ServerMessage::new(&text))),
      ObjectMessagePayload::Custom { name, payload } => {
        let _ = self
          .lua_state
          .context(|lua_ctx| {
            let main: rlua::Function = lua_ctx.globals().get("main")?;
            main.call((name, payload))?;
            Ok(())
          })
          .map_err(|err: rlua::Error| log::error!("Failed running payload: {:?}", err));
      }
    }
  }
}

impl Handler<FreezeMessage> for ObjectActor {
  type Result = Option<FreezeResponse>;

  fn handle(&mut self, _msg: FreezeMessage, _ctx: &mut Self::Context) -> Option<FreezeResponse> {
    Some(FreezeResponse {
      id: self.id,
      state: ObjectActorState {},
    })
  }
}

#[derive(Debug)]
pub struct ObjectMessage {
  pub immediate_sender: Id,
  pub payload: ObjectMessagePayload,
}

#[derive(Debug)]
pub enum ObjectMessagePayload {
  Say {
    text: String,
  },
  Tell {
    text: String,
  },
  Custom {
    name: String,
    payload: SerializableValue,
  },
}

impl Message for ObjectMessage {
  type Result = ();
}
