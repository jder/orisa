use super::WorldRef;
use crate::chat::ToClientMessage;
use crate::lua::{LuaHost, PackageReference, SerializableValue};
use crate::object::executor::ObjectExecutor;
use crate::object::types::*;
use actix;
use std::collections::HashMap;

pub struct WorldActor {
  lua_host: LuaHost,
  world_ref: WorldRef,
  executors: HashMap<PackageReference, ObjectExecutor>,
}

impl actix::Actor for WorldActor {
  type Context = actix::Context<Self>;
}

impl actix::Message for Message {
  type Result = ();
}

impl actix::Handler<Message> for WorldActor {
  type Result = ();

  fn handle(&mut self, msg: Message, _ctx: &mut actix::Context<Self>) {
    let _ = self.execute_message(&msg).map_err(|err| {
      self.report_error(&msg, &err);
      log::error!("Failed running payload: {:?}", err);
    });
  }
}

pub enum ControlMessage {
  ReloadCode,
}

impl actix::Message for ControlMessage {
  type Result = ();
}

impl actix::Handler<ControlMessage> for WorldActor {
  type Result = ();

  fn handle(&mut self, msg: ControlMessage, _ctx: &mut actix::Context<Self>) {
    match msg {
      ControlMessage::ReloadCode => {
        log::info!("clearing executor cache for code reload");
        self.executors = HashMap::new();
      }
    }
  }
}

impl WorldActor {
  pub fn new(lua_host: &LuaHost, world_ref: &WorldRef) -> WorldActor {
    WorldActor {
      lua_host: lua_host.clone(),
      world_ref: world_ref.clone(),
      executors: HashMap::new(),
    }
  }

  pub fn executor(&mut self, kind: PackageReference) -> ObjectExecutor {
    let host = &self.lua_host;
    let wf = &self.world_ref;

    self
      .executors
      .entry(kind.clone())
      .or_insert_with(|| ObjectExecutor::new(host, wf.clone()))
      .clone()
  }

  pub fn execute_message(&mut self, message: &Message) -> rlua::Result<()> {
    let kind = self
      .world_ref
      .read(|w| w.get_state().kind(message.target))?;

    let executor = self.executor(kind);
    executor.run_main(self, &message, false)?;
    Ok(())
  }

  pub fn execute_query(&mut self, message: &Message) -> rlua::Result<SerializableValue> {
    let kind = self
      .world_ref
      .read(|w| w.get_state().kind(message.target))?;

    let executor = self.executor(kind);
    executor.run_main(self, &message, true)
  }

  fn report_error(&self, msg: &Message, err: &rlua::Error) {
    if let Some(user_id) = msg.original_user {
      self.world_ref.read(|w| {
        w.send_client_message(
          user_id,
          ToClientMessage::Log {
            level: "error".to_string(),
            message: err.to_string(),
          },
        )
      });
    }
  }
}
