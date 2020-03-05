use crate::lua::{PackageReference, SerializableValue};
use core::ops::Add;
use serde::{Deserialize, Serialize};
use std::fmt;

/// We identify objects by the package their handler is implemented in.
pub type ObjectKind = PackageReference;

#[derive(Debug, PartialEq, Clone, Copy, Hash, Eq, Deserialize, Serialize)]
pub struct Id(pub usize);

impl fmt::Display for Id {
  fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
    write!(f, "#{}", self.0)
  }
}

impl<'lua> rlua::ToLua<'lua> for Id {
  fn to_lua(self, lua_ctx: rlua::Context<'lua>) -> rlua::Result<rlua::Value> {
    format!("{}", self).to_lua(lua_ctx)
  }
}

impl<'lua> rlua::FromLua<'lua> for Id {
  fn from_lua(value: rlua::Value<'lua>, _lua_ctx: rlua::Context<'lua>) -> rlua::Result<Id> {
    if let rlua::Value::String(s) = value {
      let string = s.to_str()?;
      if string.starts_with("#") {
        let index = &string[1..]
          .parse::<usize>()
          .map_err(|e| rlua::Error::external(e))?;
        Ok(Id(*index))
      } else {
        Err(rlua::Error::external("Invalid object id"))
      }
    } else {
      Err(rlua::Error::external("Expected a string for an object id"))
    }
  }
}

impl Id {
  pub fn new(id: usize) -> Id {
    Id(id)
  }
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct Message {
  pub target: Id,
  pub immediate_sender: Id,
  pub original_user: Option<Id>,
  pub name: String,
  pub payload: SerializableValue,
}
#[derive(Debug, PartialEq, PartialOrd, Clone, Copy, Hash, Eq, Deserialize, Serialize)]
pub struct GameTime(u64);

impl<'lua> rlua::ToLua<'lua> for GameTime {
  fn to_lua(self, _lua_ctx: rlua::Context<'lua>) -> rlua::Result<rlua::Value> {
    Ok(rlua::Value::Number(self.0 as f64))
  }
}

impl<'lua> rlua::FromLua<'lua> for GameTime {
  fn from_lua(value: rlua::Value<'lua>, _lua_ctx: rlua::Context<'lua>) -> rlua::Result<GameTime> {
    if let rlua::Value::Number(n) = value {
      if n > 0.0 {
        Ok(GameTime(n as u64))
      } else {
        Err(rlua::Error::external(
          "Expected positive number for game time",
        ))
      }
    } else {
      Err(rlua::Error::external("Expected a number for game time"))
    }
  }
}

impl Add<u64> for GameTime {
  type Output = GameTime;
  fn add(self, rhs: u64) -> GameTime {
    GameTime(self.0 + rhs)
  }
}

impl Default for GameTime {
  fn default() -> Self {
    return GameTime(0);
  }
}

#[derive(Serialize, Deserialize, Clone)]
pub struct Timer {
  pub target_time: GameTime,
  pub original_user: Option<Id>,
  pub message_name: String,
  pub payload: SerializableValue,
}
