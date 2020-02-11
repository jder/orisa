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

fn send_user_edit_file(
  _lua_ctx: rlua::Context,
  (name, content): (String, String),
) -> rlua::Result<()> {
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

fn get_username(_lua_ctx: rlua::Context, id: Id) -> rlua::Result<Option<String>> {
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

fn set_attr(
  _lua_ctx: rlua::Context,
  (id, key, value): (Id, String, SerializableValue),
) -> rlua::Result<SerializableValue> {
  if id != S::get_id() {
    // Someday we might relax this given capabilities and probably containment (for concurrency)
    Err(rlua::Error::external("Can only set your own attrs."))
  } else {
    Ok(
      S::with_changed_attrs(|changed_attrs| changed_attrs.insert(key.clone(), value))
        .or_else(|| S::with_world(|w| w.get_attr(id, &key)))
        .unwrap_or(SerializableValue::Nil),
    )
  }
}

fn get_attr(_lua_ctx: rlua::Context, (id, key): (Id, String)) -> rlua::Result<SerializableValue> {
  let self_id = S::get_id();
  if id == self_id {
    if let Some(changed) = S::with_changed_attrs(|attrs| attrs.get(&key).map(|v| v.clone())) {
      return Ok(changed);
    }
  }
  Ok(S::with_world(|w| w.get_attr(id, &key)).unwrap_or(SerializableValue::Nil))
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

fn send_create_object(
  _lua_ctx: rlua::Context,
  (parent, kind, created_payload): (Option<Id>, ObjectKind, SerializableValue),
) -> rlua::Result<()> {
  S::add_write(GlobalWrite::CreateObject {
    parent: parent,
    kind: kind,
    init_message: ObjectMessage {
      original_user: S::get_original_user(),
      immediate_sender: S::get_id(),
      name: "created".to_string(),
      payload: created_payload,
    },
  });

  Ok(())
}

fn send_move_object(
  _lua_ctx: rlua::Context,
  (child, new_parent): (Id, Option<Id>),
) -> rlua::Result<()> {
  if child != S::get_id() {
    return Err(rlua::Error::external("only an object can move itself"));
  }
  S::add_write(GlobalWrite::MoveObject {
    child: child,
    new_parent: new_parent,
  });
  Ok(())
}

fn print_override<'lua>(
  lua_ctx: rlua::Context<'lua>,
  vals: rlua::Variadic<rlua::Value<'lua>>,
) -> rlua::Result<()> {
  if let (Some(user_id), id, message_name) = S::with_state(|s| {
    (
      s.current_message.original_user,
      s.id,
      s.current_message.name.clone(),
    )
  }) {
    let mut result = format!("{} (for {}): ", id, message_name).to_string();
    for v in vals.iter() {
      if let Some(s) = lua_ctx.coerce_string(v.clone())? {
        result.push_str(s.to_str()?);
        result.push_str(" ");
      } else {
        return Err(rlua::Error::external(
          "Unable to convert value to string for printing",
        ));
      }
    }

    S::with_world(|w| {
      w.send_client_message(
        user_id,
        ToClientMessage::Log {
          level: "info".to_string(),
          message: result,
        },
      )
    });
  }

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
  orisa.set(
    "send_user_edit_file",
    lua_ctx.create_function(send_user_edit_file)?,
  )?;
  orisa.set(
    "send_create_object",
    lua_ctx.create_function(send_create_object)?,
  )?;
  orisa.set(
    "send_move_object",
    lua_ctx.create_function(send_move_object)?,
  )?;

  orisa.set("get_children", lua_ctx.create_function(get_children)?)?;
  orisa.set("get_parent", lua_ctx.create_function(get_parent)?)?;
  orisa.set("get_username", lua_ctx.create_function(get_username)?)?;
  orisa.set("get_kind", lua_ctx.create_function(get_kind)?)?;
  orisa.set("set_state", lua_ctx.create_function(set_state)?)?;
  orisa.set("get_state", lua_ctx.create_function(get_state)?)?;
  orisa.set("set_attr", lua_ctx.create_function(set_attr)?)?;
  orisa.set("get_attr", lua_ctx.create_function(get_attr)?)?;

  orisa.set(
    "get_custom_space_content",
    lua_ctx.create_function(get_custom_space_content)?,
  )?;
  orisa.set(
    "send_save_custom_space_content",
    lua_ctx.create_function(send_save_custom_space_content)?,
  )?;

  globals.set("orisa", orisa)?;

  globals.set("print", lua_ctx.create_function(print_override)?)?;

  Ok(())
}
