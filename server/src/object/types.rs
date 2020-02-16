use crate::lua::PackageReference;
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
