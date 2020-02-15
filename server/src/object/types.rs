use serde::{Deserialize, Serialize};
use std::fmt;

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

#[derive(Clone, Debug, Hash, PartialEq, Eq, Serialize, Deserialize)]
#[serde(from = "String", into = "String")]
pub struct ObjectKind {
  components: Vec<String>,
}

impl ObjectKind {
  pub fn new(name: &str) -> ObjectKind {
    let result = ObjectKind {
      components: name.split(".").map(|s| s.to_string()).collect(),
    };
    // TODO: nicer error handling
    assert!(result.components.len() == 2);
    result
  }

  pub fn user(username: &str) -> ObjectKind {
    ObjectKind::new(&format!("{}.user", username))
  }

  pub fn room() -> ObjectKind {
    ObjectKind::new("system.room")
  }

  pub fn top_level_package(&self) -> &str {
    &self.components.first().unwrap()
  }

  pub fn system_package() -> &'static str {
    return "system";
  }
}

impl std::fmt::Display for ObjectKind {
  fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
    f.write_str(&self.components.join("."))
  }
}

impl From<String> for ObjectKind {
  fn from(s: String) -> ObjectKind {
    return ObjectKind::new(&s);
  }
}

impl Into<String> for ObjectKind {
  fn into(self) -> String {
    return self.to_string();
  }
}

impl<'lua> rlua::ToLua<'lua> for ObjectKind {
  fn to_lua(self, lua_ctx: rlua::Context<'lua>) -> rlua::Result<rlua::Value> {
    self.to_string().to_lua(lua_ctx)
  }
}

impl<'lua> rlua::FromLua<'lua> for ObjectKind {
  fn from_lua(value: rlua::Value<'lua>, _lua_ctx: rlua::Context<'lua>) -> rlua::Result<ObjectKind> {
    // TODO: more validation
    if let rlua::Value::String(s) = value {
      let string = s.to_str()?;
      Ok(ObjectKind::new(string))
    } else {
      Err(rlua::Error::external(
        "Expected a string for an object kind",
      ))
    }
  }
}
