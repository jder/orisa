use rlua;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::fs::File;
use std::io::Read;
use std::path::{Path, PathBuf};

#[derive(Clone)]
pub struct LuaHost {
  root: PathBuf,
}

impl LuaHost {
  pub fn fresh_state(&self) -> rlua::Result<rlua::Lua> {
    let libs = rlua::StdLib::BASE
      | rlua::StdLib::COROUTINE
      | rlua::StdLib::TABLE
      | rlua::StdLib::STRING
      | rlua::StdLib::UTF8
      | rlua::StdLib::MATH;
    let lua = rlua::Lua::new_with(libs);
    lua.context(|lua_ctx| {
      // remove some sensitive things, replace load with a string-only version
      lua_ctx.globals().set("dofile", rlua::Value::Nil)?;
      lua_ctx.globals().set("loadfile", rlua::Value::Nil)?;
      lua_ctx.globals().set("collectgarbage", rlua::Value::Nil)?;
      lua_ctx
        .globals()
        .set("load", lua_ctx.create_function(LuaHost::load_string)?)?;
      Ok(())
    })?;
    Ok(lua)
  }

  pub fn load_string<'lua>(
    lua_ctx: rlua::Context<'lua>,
    (source, chunk_name, _mode, env): (
      rlua::Value<'lua>,
      Option<String>,
      Option<String>,
      Option<rlua::Table<'lua>>,
    ),
  ) -> rlua::Result<rlua::Function<'lua>> {
    let text = match source {
      rlua::Value::String(s) => s.to_str()?.to_string(),
      rlua::Value::Function(f) => {
        let mut t = String::new();
        loop {
          let res = f.call::<_, Option<String>>(())?;
          match res {
            None => break,
            Some(s) => t.push_str(&s),
          }
        }
        t
      }
      _ => {
        return Err(rlua::Error::external(format!(
          "Expected load_string source to be string or function, got {:?}",
          source
        )))
      }
    };

    let mut chunk = lua_ctx.load(&text);

    if let Some(n) = chunk_name {
      chunk = chunk.set_name(&n)?;
    }

    if let Some(e) = env {
      chunk = chunk.set_environment(e)?;
    }

    chunk.into_function()
  }

  // load a system package (e.g. loads system.main when you pass a name of "main")
  pub fn load_system_package<'lua>(
    &self,
    lua_ctx: rlua::Context<'lua>,
    name: &str,
  ) -> rlua::Result<rlua::Value<'lua>> {
    let content = self
      .system_package_to_buf(name)
      .map_err(|e| rlua::Error::external(format!("Loading system package {}: {}", name, e)))?;
    lua_ctx
      .load(&content)
      .set_name(&format!("system package {}", name))?
      .eval()
      .map_err(|e| {
        log::error!("Error loading system package {}: {}", name, e);
        e
      })
  }

  // Supports loading modules out of the top level of the system directory
  // i.e. allows loading system.main if you pass `system_package_to_buf("main")`
  fn system_package_to_buf(&self, name: &str) -> std::io::Result<Vec<u8>> {
    let mut filename = name.to_string();
    filename.push_str(".lua");
    let path = self.root.join(Path::new(&filename)).canonicalize()?;
    if !path.starts_with(&self.root) {
      log::warn!(
        "Trying to require {:?} but outside of root {:?}",
        path,
        &self.root
      );
      Err(std::io::Error::new(
        std::io::ErrorKind::Other,
        "Can't require outside of root",
      ))
    } else {
      LuaHost::unchecked_path_to_buf(&path)
    }
  }

  pub fn new(root: &Path) -> std::io::Result<LuaHost> {
    let canonical_root = root.to_path_buf().canonicalize()?;
    Ok(LuaHost {
      root: canonical_root.clone(),
    })
  }

  fn unchecked_path_to_buf(p: &Path) -> std::io::Result<Vec<u8>> {
    let mut f = File::open(p)?;
    let mut v: Vec<u8> = vec![];
    f.read_to_end(&mut v)?;
    Ok(v)
  }
}

#[derive(Serialize, Deserialize, Clone, Debug)]
#[serde(untagged)]
pub enum SerializableValue {
  Nil,
  Boolean(bool),
  Integer(i64),
  Number(f64),
  String(String),
  Table(Vec<(SerializableValue, SerializableValue)>),
  Dict(HashMap<String, SerializableValue>), // for JSON compat
}

impl<'lua> rlua::FromLua<'lua> for SerializableValue {
  fn from_lua(
    lua_value: rlua::Value<'lua>,
    _lua: rlua::Context<'lua>,
  ) -> rlua::Result<SerializableValue> {
    match lua_value {
      rlua::Value::Nil => Ok(SerializableValue::Nil),
      rlua::Value::Boolean(b) => Ok(SerializableValue::Boolean(b)),
      rlua::Value::Integer(i) => Ok(SerializableValue::Integer(i)),
      rlua::Value::Number(n) => Ok(SerializableValue::Number(n)),
      rlua::Value::String(s) => Ok(SerializableValue::String(s.to_str()?.to_string())),
      rlua::Value::Table(t) => {
        let pairs = t
          .pairs()
          .collect::<Vec<rlua::Result<(SerializableValue, SerializableValue)>>>();
        if let Some(error) = pairs.iter().find(|r| r.is_err()) {
          Err(error.as_ref().unwrap_err().clone())
        } else {
          Ok(SerializableValue::Table(
            pairs.into_iter().map(|r| r.unwrap()).collect(),
          ))
        }
      }
      // this nonsense is all because the typename method is private
      rlua::Value::Function { .. } => Err(rlua::Error::FromLuaConversionError {
        from: "function",
        to: "SerializableValue",
        message: None,
      }),
      rlua::Value::UserData { .. } => Err(rlua::Error::FromLuaConversionError {
        from: "userdata",
        to: "SerializableValue",
        message: None,
      }),
      rlua::Value::LightUserData { .. } => Err(rlua::Error::FromLuaConversionError {
        from: "light userdata",
        to: "SerializableValue",
        message: None,
      }),
      rlua::Value::Thread { .. } => Err(rlua::Error::FromLuaConversionError {
        from: "thread",
        to: "SerializableValue",
        message: None,
      }),
      rlua::Value::Error { .. } => Err(rlua::Error::FromLuaConversionError {
        from: "error",
        to: "SerializableValue",
        message: None,
      }),
    }
  }
}

impl<'lua> rlua::ToLua<'lua> for SerializableValue {
  fn to_lua(self, lua: rlua::Context<'lua>) -> rlua::Result<rlua::Value<'lua>> {
    match self {
      SerializableValue::Nil => Ok(rlua::Value::Nil),
      SerializableValue::Boolean(b) => Ok(rlua::Value::Boolean(b)),
      SerializableValue::Integer(i) => Ok(rlua::Value::Integer(i)),
      SerializableValue::Number(n) => Ok(rlua::Value::Number(n)),
      SerializableValue::String(s) => Ok(s.to_lua(lua)?),
      SerializableValue::Table(pairs) => lua
        .create_table_from(pairs.into_iter())
        .map(|t| rlua::Value::Table(t)),
      SerializableValue::Dict(dict) => lua
        .create_table_from(dict.into_iter())
        .map(|t| rlua::Value::Table(t)),
    }
  }
}
