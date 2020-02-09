use rlua;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::fs::File;
use std::io::Read;
use std::path::{Path, PathBuf};

#[derive(Clone)]
pub struct LuaHost {
  root: PathBuf,
  extra_source: Option<String>,
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

      // simplified module loading
      let root_path = self.root.clone();
      let package = lua_ctx.create_table()?;
      package.set("loaded", lua_ctx.create_table()?)?;
      lua_ctx.globals().set("package", package)?;
      lua_ctx.globals().set(
        "require",
        lua_ctx.create_function(move |lua_ctx, name: String| {
          let loaded = lua_ctx
            .globals()
            .get::<_, rlua::Table>("package")?
            .get::<_, rlua::Table>("loaded")?;
          let existing = loaded.get::<_, rlua::Value>(name.clone())?;
          if let rlua::Value::Nil = existing {
            match LuaHost::require(root_path.clone(), &name) {
              Err(io_err) => Err(rlua::Error::external(io_err)),
              Ok(bytes) => lua_ctx.load(&bytes).eval().and_then(|v: rlua::Value| {
                let maybe_populated = loaded.get::<_, rlua::Value>(name.clone())?;
                if let rlua::Value::Nil = maybe_populated {
                  loaded.set(name.to_string(), v.clone())?;
                  Ok(v)
                } else {
                  Ok(maybe_populated)
                }
              }),
            }
          } else {
            Ok(existing)
          }
        })?,
      )?;

      match &self.extra_source {
        Some(content) => lua_ctx.load(content).exec()?,
        None => lua_ctx
          .load(
            &LuaHost::load_unchecked_path(&self.root.join(Path::new("main.lua")))
              .map_err(|e| rlua::Error::external(e))?,
          )
          .exec()?,
      };

      Ok(())
    })?;
    Ok(lua)
  }

  pub fn with_source(&self, source: &str) -> LuaHost {
    LuaHost {
      root: self.root.clone(),
      extra_source: Some(source.to_string()),
    }
  }

  fn load_string(lua_ctx: rlua::Context, source: String) -> rlua::Result<rlua::Function> {
    lua_ctx.load(&source).into_function()
  }

  // For now, only allow foo.lua in the same folder.
  // Later we should permit `system.bar` and `user/repo.bar`
  fn require(root: PathBuf, name: &str) -> std::io::Result<Vec<u8>> {
    let mut filename = name.to_string();
    filename.push_str(".lua");
    let path = root.join(Path::new(&filename)).canonicalize()?;
    if !path.starts_with(&root) {
      log::warn!(
        "Trying to require {:?} but outside of root {:?}",
        path,
        &root
      );
      Err(std::io::Error::new(
        std::io::ErrorKind::Other,
        "Can't require outside of root",
      ))
    } else {
      LuaHost::load_unchecked_path(&path)
    }
  }

  pub fn new(root: &Path) -> std::io::Result<LuaHost> {
    let canonical_root = root.to_path_buf().canonicalize()?;
    Ok(LuaHost {
      root: canonical_root.clone(),
      extra_source: None,
    })
  }

  fn load_unchecked_path(p: &Path) -> std::io::Result<Vec<u8>> {
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
