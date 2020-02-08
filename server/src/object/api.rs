use crate::object::actor::{ObjectActor, ObjectActorState};
use crate::world::{Id, World, WorldRef};
use rlua;
use std::cell::RefCell;

pub struct ExecutionState<'a> {
  id: Id,
  world: WorldRef,
  state: RefCell<&'a mut ObjectActorState>,
}

impl<'a> ExecutionState<'a> {
  fn with_state<T, F>(body: F) -> T
  where
    F: FnOnce(&ExecutionState) -> T,
  {
    EXECUTION_STATE.with(|s| body(s))
  }

  fn with_actor_state_mut<T, F>(body: F) -> T
  where
    F: FnOnce(&mut ObjectActorState) -> T,
  {
    Self::with_state(|s| body(&mut s.state.borrow_mut()))
  }

  fn with_world<T, F>(body: F) -> T
  where
    F: FnOnce(&World) -> T,
  {
    Self::with_state(|s| s.world.read(|w| body(w)))
  }

  fn get_id() -> Id {
    Self::with_state(|s| s.id)
  }
}

scoped_thread_local! {static EXECUTION_STATE: ExecutionState}

// API

mod api {
  use crate::chat::{ChatRowContent, ToClientMessage};
  use crate::lua::*;
  use crate::object::actor::ObjectMessage;
  use crate::object::api::ExecutionState as S;
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
}

pub fn register_api(lua_ctx: rlua::Context) -> rlua::Result<()> {
  let globals = lua_ctx.globals();
  let orisa = lua_ctx.create_table()?;

  orisa.set("get_children", lua_ctx.create_function(api::get_children)?)?;
  orisa.set("send", lua_ctx.create_function(api::send)?)?;
  orisa.set("tell", lua_ctx.create_function(api::tell)?)?;
  orisa.set("get_name", lua_ctx.create_function(api::get_name)?)?;
  orisa.set("get_kind", lua_ctx.create_function(api::get_kind)?)?;
  orisa.set("set_state", lua_ctx.create_function(api::set_state)?)?;
  orisa.set("get_state", lua_ctx.create_function(api::get_state)?)?;

  globals.set("orisa", orisa)?;
  Ok(())
}

pub fn with_api<'a, F, T>(actor: &mut ObjectActor, body: F) -> T
where
  F: FnOnce(rlua::Context) -> T,
{
  let state = ExecutionState {
    id: actor.id,
    world: actor.world.clone(),
    state: RefCell::new(&mut actor.state),
  };

  let lua_state = &actor.lua_state;

  // This is a gross hack but is safe since the scoped thread local ensures
  // this value only exists as long as this block.
  EXECUTION_STATE.set(unsafe { make_static(&state) }, || {
    lua_state.context(|lua_ctx| body(lua_ctx))
  })
}

unsafe fn make_static<'a>(p: &'a ExecutionState<'a>) -> &'static ExecutionState<'static> {
  use std::mem;
  mem::transmute(p)
}
