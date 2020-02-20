use crate::chat::{ChatRowContent, ToClientMessage};
use crate::lua::*;
use crate::object::executor::ExecutionState as S;
use crate::object::types::*;
use crate::world::actor::WorldActor;
use rlua;
use rlua::ExternalResult;
use rlua::ToLua;
use std::collections::HashMap;

fn get_children(_lua_ctx: rlua::Context, object_id: Id) -> rlua::Result<Vec<Id>> {
  Ok(S::with_world_state(|w| {
    w.children(object_id).collect::<Vec<Id>>()
  }))
}

fn get_parent(_lua_ctx: rlua::Context, object_id: Id) -> rlua::Result<Option<Id>> {
  Ok(S::with_world_state(|w| w.parent(object_id))?)
}

fn send(
  _lua_ctx: rlua::Context,
  (object_id, name, payload): (Id, String, SerializableValue),
) -> rlua::Result<()> {
  S::with_world_mut(|w| {
    Ok(w.send_message(Message {
      target: object_id,
      original_user: S::get_original_user(),
      immediate_sender: S::get_id(),
      name: name,
      payload: payload,
    }))
  })
}

fn query(
  lua_ctx: rlua::Context,
  (object_id, name, payload): (Id, String, SerializableValue),
) -> rlua::Result<SerializableValue> {
  S::with_state_mut(|s| {
    let id = s.current_message.target;

    if s.in_query {
      // TODO: lift this restriction once we can re-use executors or have a pool of them
      return Err(rlua::Error::external(
        "You currently can't run a query from a query, sorry.",
      ));
    }
    if object_id == id {
      // TODO: lift this restriction once we can re-use executors or have a pool of them
      // (Or call directly on lua_ctx with the new message state.)
      return Err(rlua::Error::external(
        "You currently can't query yourself, sorry.",
      ));
    }
    let result = s.actor.execute_query(&Message {
      target: object_id,
      immediate_sender: id,
      original_user: s.current_message.original_user,
      name: name.clone(),
      payload: payload.clone(),
    });

    // Restore current message before returning control to the caller
    WorldActor::set_globals_for_message(&lua_ctx, s.current_message)
      .expect("Unable to restore previous globals");

    result
  })
}

fn send_user_tell(_lua_ctx: rlua::Context, message: String) -> rlua::Result<()> {
  S::with_world_mut(|w| {
    Ok(w.send_client_message(
      S::get_id(),
      ToClientMessage::Tell {
        content: ChatRowContent::new(&message),
      },
    ))
  })
}

fn send_user_tell_html(_lua_ctx: rlua::Context, html: String) -> rlua::Result<()> {
  S::with_world_mut(|w| {
    Ok(w.send_client_message(
      S::get_id(),
      ToClientMessage::Tell {
        content: ChatRowContent::new_html(&html),
      },
    ))
  })
}

fn send_user_backlog(_lua_ctx: rlua::Context, messages: Vec<String>) -> rlua::Result<()> {
  S::with_world_mut(|w| {
    Ok(w.send_client_message(
      S::get_id(),
      ToClientMessage::Backlog {
        history: messages.iter().map(|s| ChatRowContent::new(s)).collect(),
      },
    ))
  })
}

fn send_user_edit_file(
  _lua_ctx: rlua::Context,
  (name, content): (String, String),
) -> rlua::Result<()> {
  S::with_world_mut(|w| {
    Ok(w.send_client_message(
      S::get_id(),
      ToClientMessage::EditFile {
        name: name,
        content: content,
      },
    ))
  })
}

fn get_username(_lua_ctx: rlua::Context, id: Id) -> rlua::Result<Option<String>> {
  Ok(S::with_world_state(|w| w.username(id)))
}

fn get_kind(lua_ctx: rlua::Context, id: Id) -> rlua::Result<rlua::Value> {
  S::with_world_state(|w| w.kind(id)?.to_lua(lua_ctx))
}

fn set_state(
  _lua_ctx: rlua::Context,
  (id, key, value): (Id, String, SerializableValue),
) -> rlua::Result<SerializableValue> {
  if id != S::get_id() {
    // Someday we might relax this given capabilities and probably containment (for concurrency)
    Err(rlua::Error::external("Can only set your own state."))
  } else {
    S::with_world_state_mut::<SerializableValue, _>(|s| {
      Ok(
        s.set_state(id, &key, value)?
          .unwrap_or(SerializableValue::Nil),
      )
    })
  }
}

fn get_state(_lua_ctx: rlua::Context, (id, key): (Id, String)) -> rlua::Result<SerializableValue> {
  if id != S::get_id() {
    // Someday we might relax this given capabilities and probably containment (for concurrency)
    Err(rlua::Error::external("Can only get your own state."))
  } else {
    Ok(S::with_world_state(|s| s.get_state(id, &key))?.unwrap_or(SerializableValue::Nil))
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
      S::with_world_state_mut(|s| Ok(s.set_attr(id, key.clone(), value)?))?
        .unwrap_or(SerializableValue::Nil),
    )
  }
}

fn get_attr(_lua_ctx: rlua::Context, (id, key): (Id, String)) -> rlua::Result<SerializableValue> {
  Ok(S::with_world_state(|w| w.get_attr(id, &key))?.unwrap_or(SerializableValue::Nil))
}

fn get_live_package_content(_lua_ctx: rlua::Context, name: String) -> rlua::Result<Option<String>> {
  S::with_world_state(|w| {
    PackageReference::new(&name)
      .map(|package| w.get_live_package_content(package).map(|s| s.clone()))
  })
  .map_err(|e| rlua::Error::external(e))
}

fn send_save_live_package_content(
  _lua_ctx: rlua::Context,
  (name, content): (String, String),
) -> rlua::Result<()> {
  let destination_package = PackageReference::new(&name).to_lua_err()?;
  let id = S::get_id();

  if Some(destination_package.user().to_string()) == S::with_world_state(|w| w.username(id))
    && destination_package.is_live_package()
  {
    S::with_world_state_mut(|s| {
      Ok(s.set_live_package_content(PackageReference::new(&name).to_lua_err()?, content))
    })?;
    // TODO: reload only this package
    S::with_world_mut(|w| Ok(w.reload_code()))
  } else {
    Err(rlua::Error::external(
      "You can only write to live packages named $username/live.something",
    ))
  }
}

// This is a bit of a special case.
// We allow creation of an object immediately even though this has side effects
// visible in the rest of the world. Practically, though, since we create it
// with no parent, it will not meaningfully change anyone else that is running,
// so long as they do not assume consecutive object ids.
fn create_object(
  _lua_ctx: rlua::Context,
  (parent, kind, created_payload): (Option<Id>, ObjectKind, SerializableValue),
) -> rlua::Result<Id> {
  S::with_world_mut(|w| {
    let id = w.get_state_mut().create_object(kind);
    w.get_state_mut().move_object(id, parent)?;
    w.send_message(Message {
      target: id,
      original_user: S::get_original_user(),
      immediate_sender: S::get_id(),
      name: "created".to_string(),
      payload: created_payload,
    });
    Ok(id)
  })
}

fn find_room(a: Id) -> rlua::Result<Id> {
  let parent = S::with_world_state(|w| w.parent(a))?;
  match parent {
    None => Ok(a),
    Some(p) => find_room(p),
  }
}

fn shares_room(a: Id, b: Id) -> rlua::Result<bool> {
  let room_a = find_room(a)?;
  let room_b = find_room(b)?;
  Ok(room_a == room_b)
}

fn move_object(_lua_ctx: rlua::Context, (child, new_parent): (Id, Option<Id>)) -> rlua::Result<()> {
  let sender = S::get_id();
  // TODO: this check should move to a lua query on the child and/or new/old parent
  if child != sender && !shares_room(child, sender)? {
    return Err(rlua::Error::external(
      "only something in the same room or the object itself can move an object",
    ));
  }

  // TODO: this boilerplate is horrible; surely we can do something nicer
  // TODO: we probably want to build these at commit time so we can read old_parent
  // and do whatever permission checks based on current location.
  let mut info: HashMap<String, SerializableValue> = HashMap::new();
  info.insert(
    "child".to_string(),
    SerializableValue::String(child.to_string()),
  );
  info.insert(
    "new_parent".to_string(),
    new_parent
      .map(|p| SerializableValue::String(p.to_string()))
      .unwrap_or(SerializableValue::Nil),
  );

  let payload = SerializableValue::Dict(info);
  let original_user = S::get_original_user();
  let id = S::get_id();

  S::with_world_mut(|w| {
    w.get_state_mut().move_object(child, new_parent)?;
    w.send_message(Message {
      target: child,
      original_user: original_user,
      immediate_sender: id,
      name: "parent_changed".to_string(),
      payload: payload.clone(),
    });

    new_parent.map(|p| {
      w.send_message(Message {
        target: p,
        original_user: original_user,
        immediate_sender: id,
        name: "child_added".to_string(),
        payload: payload.clone(),
      });
    });
    Ok(())
  })
}

fn print_override<'lua>(
  lua_ctx: rlua::Context<'lua>,
  vals: rlua::Variadic<rlua::Value<'lua>>,
) -> rlua::Result<()> {
  let (maybe_user_id, id, message_name) = S::with_state(|s| {
    (
      s.current_message.original_user,
      s.current_message.target,
      s.current_message.name.clone(),
    )
  });
  let mut result = format!("{} (for {}): ", id, message_name).to_string();
  for v in vals.iter() {
    let piece = match lua_ctx.coerce_string(v.clone())? {
      Some(lua_str) => lua_str.to_str()?.to_string(),
      None => format!("{:?}", v),
    };

    result.push_str(&piece);
    result.push_str(" ");
  }

  log::info!("lua: {}", result);

  if let Some(user_id) = maybe_user_id {
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

// We currently load packages in 2 flavours:
// * system.foo, which loads "foo.lua" from the filesystem.
// * user/live.foo, which loads the local (in-memory) package named user.foo from the world.
// In the future, we want to extend this to user/repo.foo
fn require(lua_ctx: rlua::Context, package_name: String) -> rlua::Result<rlua::Value> {
  let loaded = lua_ctx
    .globals()
    .get::<_, rlua::Table>("package")?
    .get::<_, rlua::Table>("loaded")?;
  let existing = loaded.get::<_, rlua::Value>(package_name.clone())?;
  if let rlua::Value::Nil = existing {
    // Load the package
    let package_reference = PackageReference::new(&package_name).to_lua_err()?;

    let package = if package_reference.is_live_package() {
      S::with_world_state(|w| {
        let content = w
          .get_live_package_content(PackageReference::new(&package_name).to_lua_err()?)
          .ok_or(rlua::Error::external(format!(
            "Can't find local package {}",
            package_name
          )))?;
        lua_ctx
          .load(content)
          .set_name(&package_reference.to_string())?
          .eval()
      })
    } else if package_reference.package_root()
      == PackageReference::system_package_root().to_string()
    {
      // from the system
      S::with_world(|w| {
        w.get_lua_host()
          .load_filesystem_package(lua_ctx, &package_reference)
      })
    } else {
      return Err(rlua::Error::external(
        "Only the system or live repos are currently supported.",
      ));
    };

    package.and_then(|v: rlua::Value| {
      let maybe_populated = loaded.get::<_, rlua::Value>(package_name.clone())?;
      if let rlua::Value::Nil = maybe_populated {
        loaded.set(package_name.to_string(), v.clone())?;
        Ok(v)
      } else {
        Ok(maybe_populated)
      }
    })
  } else {
    Ok(existing)
  }
}

pub(super) fn register_api(lua_ctx: rlua::Context) -> rlua::Result<()> {
  let globals = lua_ctx.globals();
  let orisa = lua_ctx.create_table()?;

  orisa.set("send", lua_ctx.create_function(send)?)?;
  orisa.set("query", lua_ctx.create_function(query)?)?;
  orisa.set("send_user_tell", lua_ctx.create_function(send_user_tell)?)?;
  orisa.set(
    "send_user_tell_html",
    lua_ctx.create_function(send_user_tell_html)?,
  )?;
  orisa.set(
    "send_user_backlog",
    lua_ctx.create_function(send_user_backlog)?,
  )?;
  orisa.set(
    "send_user_edit_file",
    lua_ctx.create_function(send_user_edit_file)?,
  )?;
  orisa.set("move_object", lua_ctx.create_function(move_object)?)?;

  orisa.set("get_children", lua_ctx.create_function(get_children)?)?;
  orisa.set("get_parent", lua_ctx.create_function(get_parent)?)?;
  orisa.set("get_username", lua_ctx.create_function(get_username)?)?;
  orisa.set("get_kind", lua_ctx.create_function(get_kind)?)?;
  orisa.set("set_state", lua_ctx.create_function(set_state)?)?;
  orisa.set("get_state", lua_ctx.create_function(get_state)?)?;
  orisa.set("set_attr", lua_ctx.create_function(set_attr)?)?;
  orisa.set("get_attr", lua_ctx.create_function(get_attr)?)?;

  orisa.set(
    "get_live_package_content",
    lua_ctx.create_function(get_live_package_content)?,
  )?;
  orisa.set(
    "send_save_live_package_content",
    lua_ctx.create_function(send_save_live_package_content)?,
  )?;

  orisa.set("create_object", lua_ctx.create_function(create_object)?)?;

  globals.set("orisa", orisa)?;

  // Package loading mimicing the built-in lua behavior
  let package = lua_ctx.create_table()?;
  package.set("loaded", lua_ctx.create_table()?)?;
  lua_ctx.globals().set("package", package)?;
  lua_ctx
    .globals()
    .set("require", lua_ctx.create_function(require)?)?;

  globals.set("print", lua_ctx.create_function(print_override)?)?;

  Ok(())
}
