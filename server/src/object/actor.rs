use crate::chat::ToClientMessage;
use crate::lua::SerializableValue;
use crate::object::executor::ObjectExecutor;
use crate::world::*;
use actix::{Actor, Context, Handler, Message};
use rlua;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

pub struct ObjectActor {
  id: Id,
  world: WorldRef,
  state: ObjectActorState,
}

#[derive(Serialize, Deserialize, Clone)]
pub struct ObjectActorState {
  pub(super) persistent_state: HashMap<String, SerializableValue>,
}

impl ObjectActor {
  pub fn new(id: Id, world_ref: WorldRef, state: Option<ObjectActorState>) -> ObjectActor {
    ObjectActor {
      id: id,
      world: world_ref,
      state: state.unwrap_or(ObjectActorState {
        persistent_state: HashMap::new(),
      }),
    }
  }

  fn run_main(&mut self, msg: &ObjectMessage) -> rlua::Result<()> {
    let wf = self.world.clone();
    let id = self.id;

    ObjectExecutor::run_for_object(self.world.clone(), id, &msg, &mut self.state, |lua_ctx| {
      let globals = lua_ctx.globals();
      let orisa: rlua::Table = globals.get("orisa")?;
      orisa.set("self", id)?;
      let main: rlua::Function = globals.get("main")?;

      let kind = wf.read(|w| w.kind(id));
      main.call::<_, ()>((
        kind,
        msg.immediate_sender,
        msg.name.clone(),
        msg.payload.clone(),
      ))
    })
  }

  fn report_error(&self, msg: &ObjectMessage, err: &rlua::Error) {
    if let Some(user_id) = msg.original_user {
      self.world.read(|w| {
        w.send_client_message(
          user_id,
          ToClientMessage::Log {
            message: format!("Error: {}", err).to_string(),
          },
        )
      });
    }
  }
}

impl Actor for ObjectActor {
  type Context = Context<Self>;

  fn started(&mut self, _ctx: &mut Self::Context) {}
}

impl Handler<ObjectMessage> for ObjectActor {
  type Result = ();

  fn handle(&mut self, msg: ObjectMessage, _ctx: &mut Self::Context) {
    let _ = self.run_main(&msg).map_err(|err: rlua::Error| {
      self.report_error(&msg, &err);
      log::error!("Failed running payload: {:?}", err);
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

#[derive(Debug, Clone)]
pub struct ObjectMessage {
  pub immediate_sender: Id,
  pub original_user: Option<Id>,
  pub name: String,
  pub payload: SerializableValue,
}

impl Message for ObjectMessage {
  type Result = ();
}
