use crate::chat::{ChatRowContent, ToClientMessage};
use crate::lua::*;
use crate::object::actor::ObjectMessage;
use crate::object::executor::ExecutionState as S;
use crate::world::Id;

pub fn get_children(_lua_ctx: rlua::Context, object_id: Id) -> rlua::Result<Vec<Id>> {
  Ok(S::with_world(|w| {
    w.children(object_id).collect::<Vec<Id>>()
  }))
}

pub fn send(
  _lua_ctx: rlua::Context,
  (object_id, name, payload): (Id, String, SerializableValue),
) -> rlua::Result<()> {
  Ok(S::with_world(|w| {
    w.send_message(
      object_id,
      ObjectMessage {
        immediate_sender: S::get_id(),
        name: name,
        payload: payload,
      },
    )
  }))
}

pub fn tell(_lua_ctx: rlua::Context, message: String) -> rlua::Result<()> {
  Ok(S::with_world(|w| {
    w.send_client_message(
      S::get_id(),
      ToClientMessage::Tell {
        content: ChatRowContent::new(&message),
      },
    )
  }))
}

pub fn get_name(_lua_ctx: rlua::Context, id: Id) -> rlua::Result<String> {
  Ok(S::with_world(|w| w.username(id)))
}

pub fn get_kind(_lua_ctx: rlua::Context, id: Id) -> rlua::Result<String> {
  Ok(S::with_world(|w| w.kind(id).0))
}

pub fn set_state(
  _lua_ctx: rlua::Context,
  (id, key, value): (Id, String, SerializableValue),
) -> rlua::Result<SerializableValue> {
  if id != S::get_id() {
    // Someday we might relax this given capabilities and probably containment (for concurrency)
    Err(rlua::Error::external("Can only set your own state."))
  } else {
    Ok(
      S::with_actor_state_mut(|s| s.persistent_state.insert(key, value))
        .unwrap_or(SerializableValue::Nil),
    )
  }
}

pub fn get_state(
  _lua_ctx: rlua::Context,
  (id, key): (Id, String),
) -> rlua::Result<SerializableValue> {
  if id != S::get_id() {
    // Someday we might relax this given capabilities and probably containment (for concurrency)
    Err(rlua::Error::external("Can only get your own state."))
  } else {
    Ok(S::with_actor_state_mut(|s| {
      s.persistent_state
        .get(&key)
        .map(|v| v.clone())
        .unwrap_or(SerializableValue::Nil)
    }))
  }
}

pub(super) fn register_api(lua_ctx: rlua::Context) -> rlua::Result<()> {
  let globals = lua_ctx.globals();
  let orisa = lua_ctx.create_table()?;

  orisa.set("get_children", lua_ctx.create_function(get_children)?)?;
  orisa.set("send", lua_ctx.create_function(send)?)?;
  orisa.set("tell", lua_ctx.create_function(tell)?)?;
  orisa.set("get_name", lua_ctx.create_function(get_name)?)?;
  orisa.set("get_kind", lua_ctx.create_function(get_kind)?)?;
  orisa.set("set_state", lua_ctx.create_function(set_state)?)?;
  orisa.set("get_state", lua_ctx.create_function(get_state)?)?;

  globals.set("orisa", orisa)?;
  Ok(())
}
