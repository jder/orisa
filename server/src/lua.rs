use crate::world::Id;
use rlua;
use serde::{Deserialize, Serialize};
use std::fs::File;
use std::io::Read;

#[derive(Clone)]
pub struct LuaHost {
  source: Vec<u8>,
}

impl rlua::UserData for Id {}

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
      lua_ctx.load(&self.source).exec()?;
      Ok(())
    })?;
    Ok(lua)
  }

  pub fn new(p: &std::path::Path) -> std::io::Result<LuaHost> {
    let mut f = File::open(&p)?;
    let mut v: Vec<u8> = vec![];
    f.read_to_end(&mut v)?;

    Ok(LuaHost { source: v })
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
    }
  }
}
