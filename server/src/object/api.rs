use crate::chat::{ChatRowContent, ToClientMessage};
use crate::lua::*;
use crate::object::actor::ObjectMessage;
use crate::object::executor::{ExecutionState as S, GlobalWrite};
use crate::world::{Id, ObjectKind};

fn get_children(_lua_ctx: rlua::Context, object_id: Id) -> rlua::Result<Vec<Id>> {
  Ok(S::with_world(|w| {
    w.children(object_id).collect::<Vec<Id>>()
  }))
}

fn get_parent(_lua_ctx: rlua::Context, object_id: Id) -> rlua::Result<Option<Id>> {
  Ok(S::with_world(|w| w.parent(object_id)))
}

fn send(
  _lua_ctx: rlua::Context,
  (object_id, name, payload): (Id, String, SerializableValue),
) -> rlua::Result<()> {
  let message = ObjectMessage {
    original_user: S::get_original_user(),
    immediate_sender: S::get_id(),
    name: name,
    payload: payload,
  };

  // TODO: we could optimize this by sending directly if no other writes have happened yet
  S::add_write(GlobalWrite::SendMessage {
    target: object_id,
    message: message,
  });

  Ok(())
}

fn send_user_tell(_lua_ctx: rlua::Context, message: String) -> rlua::Result<()> {
  // TODO: we could optimize this by sending directly if no other writes have happened yet
  S::add_write(GlobalWrite::SendClientMessage {
    target: S::get_id(),
    message: ToClientMessage::Tell {
      content: ChatRowContent::new(&message),
    },
  });
  Ok(())
}

fn send_user_backlog(_lua_ctx: rlua::Context, messages: Vec<String>) -> rlua::Result<()> {
  // TODO: we could optimize this by sending directly if no other writes have happened yet
  S::add_write(GlobalWrite::SendClientMessage {
    target: S::get_id(),
    message: ToClientMessage::Backlog {
      history: messages.iter().map(|s| ChatRowContent::new(s)).collect(),
    },
  });
  Ok(())
}

fn edit_file(_lua_ctx: rlua::Context, (name, content): (String, String)) -> rlua::Result<()> {
  // TODO: we could optimize this by sending directly if no other writes have happened yet
  S::add_write(GlobalWrite::SendClientMessage {
    target: S::get_id(),
    message: ToClientMessage::EditFile {
      name: name,
      content: content,
    },
  });
  Ok(())
}

fn get_name(_lua_ctx: rlua::Context, id: Id) -> rlua::Result<String> {
  Ok(S::with_world(|w| w.username(id)))
}

fn get_kind(_lua_ctx: rlua::Context, id: Id) -> rlua::Result<String> {
  Ok(S::with_world(|w| w.kind(id).0))
}

fn set_state(
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

fn get_state(_lua_ctx: rlua::Context, (id, key): (Id, String)) -> rlua::Result<SerializableValue> {
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

fn get_custom_space_content(_lua_ctx: rlua::Context, name: String) -> rlua::Result<Option<String>> {
  Ok(S::with_world(|w| {
    w.get_custom_space_content(ObjectKind::new(&name))
      .map(|s| s.clone())
  }))
}

fn send_save_custom_space_content(
  _lua_ctx: rlua::Context,
  (name, content): (String, String),
) -> rlua::Result<()> {
  S::add_write(GlobalWrite::SetCustomSpaceContent {
    kind: ObjectKind(name),
    content: content,
  });
  Ok(())
}

pub(super) fn register_api(lua_ctx: rlua::Context) -> rlua::Result<()> {
  let globals = lua_ctx.globals();
  let orisa = lua_ctx.create_table()?;

  orisa.set("send", lua_ctx.create_function(send)?)?;
  orisa.set("send_user_tell", lua_ctx.create_function(send_user_tell)?)?;
  orisa.set(
    "send_user_backlog",
    lua_ctx.create_function(send_user_backlog)?,
  )?;

  orisa.set("get_children", lua_ctx.create_function(get_children)?)?;
  orisa.set("get_parent", lua_ctx.create_function(get_parent)?)?;
  orisa.set("get_name", lua_ctx.create_function(get_name)?)?;
  orisa.set("get_kind", lua_ctx.create_function(get_kind)?)?;
  orisa.set("set_state", lua_ctx.create_function(set_state)?)?;
  orisa.set("get_state", lua_ctx.create_function(get_state)?)?;

  orisa.set("edit_file", lua_ctx.create_function(edit_file)?)?;
  orisa.set(
    "get_custom_space_content",
    lua_ctx.create_function(get_custom_space_content)?,
  )?;
  orisa.set(
    "send_save_custom_space_content",
    lua_ctx.create_function(send_save_custom_space_content)?,
  )?;

  globals.set("orisa", orisa)?;
  Ok(())
}
