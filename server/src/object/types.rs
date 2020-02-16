use crate::util::*;
use core::convert::TryFrom;
use regex::Regex;
use rlua::ExternalResult;
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
#[serde(try_from = "String", into = "String")]
/// Represents the kind of an object, which is also what code runs to handle its messages.
/// e.g. system.foo -> user of system, default repo, package foo
/// e.g. jder/live.foo.bar -> user of jder, repo live, package foo.bar
///
/// The first two components (everything up to the first dot) are known as the package root
/// and usually corresponds to a directory on disk that lua code lives in.
pub struct ObjectKind {
  user: String,
  repo: Option<String>,
  package: String,
}

impl ObjectKind {
  pub fn new(name: &str) -> ResultAnyError<ObjectKind> {
    lazy_static! {
      // TODO: For now we only support a single package component (i.e. system.foo, not system.foo.bar)
      static ref RE: Regex = Regex::new(r"^(?P<user>[[:word:]]+)(/(?P<repo>[[:word:]]+))?\.(?P<package>[[:word:]]+)$").unwrap();
    }

    RE.captures(name)
      .map(|caps| ObjectKind {
        user: caps.name("user").unwrap().as_str().to_string(),
        repo: caps.name("repo").map(|s| s.as_str().to_string()),
        package: caps.name("package").unwrap().as_str().to_string(),
      })
      .ok_or_else(|| "invalid object kind".into())
  }

  pub fn user(&self) -> &str {
    return &self.user;
  }

  pub fn for_user(username: &str) -> ObjectKind {
    ObjectKind::new(&format!("{}.user", username)).unwrap()
  }

  pub fn for_room() -> ObjectKind {
    ObjectKind::new("system.room").unwrap()
  }

  pub fn package_root(&self) -> String {
    let mut result = self.user.clone();
    if let Some(ref r) = self.repo {
      result.push('/');
      result.push_str(&r);
    }
    result
  }

  pub fn system_package_root() -> &'static str {
    return "system";
  }
}

impl std::fmt::Display for ObjectKind {
  fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
    match self.repo {
      None => write!(f, "{}.{}", self.user, self.package),
      Some(ref repo) => write!(f, "{}/{}.{}", self.user, repo, self.package),
    }
  }
}

impl TryFrom<String> for ObjectKind {
  type Error = AnyError;
  fn try_from(s: String) -> ResultAnyError<ObjectKind> {
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
      Ok(ObjectKind::new(string).to_lua_err()?)
    } else {
      Err(rlua::Error::external(
        "Expected a string for an object kind",
      ))
    }
  }
}
