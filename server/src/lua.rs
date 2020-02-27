use crate::repo::Repo;
use crate::util::*;
use core::convert::TryFrom;
use git2;
use regex::Regex;
use rlua;
use rlua::ExternalResult;
use rlua::ToLua;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::fmt;
use std::fs::File;
use std::io::Read;
use std::path::{Path, PathBuf};
#[derive(Clone)]
pub struct LuaHost {
  root: PathBuf,
  repo: Option<Repo>,
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
    lua.context::<_, rlua::Result<()>>(|lua_ctx| {
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
  ) -> rlua::Result<(rlua::Value<'lua>, rlua::Value<'lua>)> {
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

    match chunk.into_function() {
      Err(e) => Ok((rlua::Value::Nil, e.to_string().to_lua(lua_ctx)?)),
      Ok(f) => Ok((rlua::Value::Function(f), rlua::Value::Nil)),
    }
  }

  // load a system (later other filesystem) package
  pub fn load_filesystem_package<'lua>(
    &self,
    lua_ctx: rlua::Context<'lua>,
    reference: &PackageReference,
  ) -> rlua::Result<rlua::Value<'lua>> {
    let content = self.filesystem_package_to_buf(reference)?;
    lua_ctx
      .load(&content)
      .set_name(&reference.to_string())?
      .eval()
      .map_err(|e| {
        log::error!("Error loading package {}: {}", reference, e);
        e
      })
  }

  pub fn filesystem_package_to_buf(&self, reference: &PackageReference) -> rlua::Result<Vec<u8>> {
    if reference.package_root() != PackageReference::system_package_root() {
      return Err(rlua::Error::external(format!(
        "Package {} is not a system package",
        reference
      )));
    }

    let name = reference.package();

    self
      .system_package_root_to_buf(name)
      .map_err(|e| rlua::Error::external(format!("Loading package {}: {}", reference, e)))
  }

  // Supports loading modules out of the top level of the system directory
  // i.e. allows loading system.main if you pass `system_package_root_to_buf("main")`
  fn system_package_root_to_buf(&self, name: &str) -> std::io::Result<Vec<u8>> {
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

  pub fn new(root: &Path, repo: Option<Repo>) -> std::io::Result<LuaHost> {
    let canonical_root = root.to_path_buf().canonicalize()?;
    Ok(LuaHost {
      root: canonical_root.clone(),
      repo,
    })
  }

  fn unchecked_path_to_buf(p: &Path) -> std::io::Result<Vec<u8>> {
    let mut f = File::open(p)?;
    let mut v: Vec<u8> = vec![];
    f.read_to_end(&mut v)?;
    Ok(v)
  }

  pub fn fetch(&self) -> Result<String, git2::Error> {
    self
      .repo
      .as_ref()
      .map(|repo| repo.pull_latest())
      .unwrap_or(Ok("Not updating from git.".to_string()))
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

#[derive(Clone, Debug, Hash, PartialEq, Eq, Serialize, Deserialize)]
#[serde(try_from = "String", into = "String")]
/// Represents a lua package we might want to load/save either from a repo or from in-memory.
/// e.g. system.foo -> user of system, default repo, package foo
/// e.g. jder/live.foo.bar -> user of jder, repo live, package foo.bar
///
/// The first two components (everything up to the first dot) are known as the package root
/// and usually corresponds to a directory on disk that lua code lives in.
pub struct PackageReference {
  user: String,
  repo: Option<String>,
  package: String,
}

impl PackageReference {
  pub fn new(name: &str) -> ResultAnyError<PackageReference> {
    lazy_static! {
      // TODO: For now we only support a single package component (i.e. system.foo, not system.foo.bar)
      static ref RE: Regex = Regex::new(r"^(?P<user>[[:word:]]+)(/(?P<repo>[[:word:]]+))?\.(?P<package>[[:word:]]+)$").unwrap();
    }

    RE.captures(name)
      .map(|caps| PackageReference {
        user: caps.name("user").unwrap().as_str().to_string(),
        repo: caps.name("repo").map(|s| s.as_str().to_string()),
        package: caps.name("package").unwrap().as_str().to_string(),
      })
      .ok_or_else(|| "invalid object kind".into())
  }

  pub fn user(&self) -> &str {
    return &self.user;
  }

  pub fn for_user(username: &str) -> PackageReference {
    PackageReference::new(&format!("{}/live.user", username)).unwrap()
  }

  pub fn for_room() -> PackageReference {
    PackageReference::for_system("room").unwrap()
  }

  pub fn for_system(name: &str) -> ResultAnyError<PackageReference> {
    PackageReference::new(&format!("system.{}", name))
  }

  pub fn main_package() -> PackageReference {
    PackageReference::for_system("main").unwrap()
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

  pub fn is_live_package(&self) -> bool {
    self.repo.as_deref() == Some("live")
  }

  pub fn package(&self) -> &str {
    return &self.package;
  }
}

impl std::fmt::Display for PackageReference {
  fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
    match self.repo {
      None => write!(f, "{}.{}", self.user, self.package),
      Some(ref repo) => write!(f, "{}/{}.{}", self.user, repo, self.package),
    }
  }
}

impl TryFrom<String> for PackageReference {
  type Error = AnyError;
  fn try_from(s: String) -> ResultAnyError<PackageReference> {
    return PackageReference::new(&s);
  }
}

impl Into<String> for PackageReference {
  fn into(self) -> String {
    return self.to_string();
  }
}

impl<'lua> rlua::ToLua<'lua> for PackageReference {
  fn to_lua(self, lua_ctx: rlua::Context<'lua>) -> rlua::Result<rlua::Value> {
    self.to_string().to_lua(lua_ctx)
  }
}

impl<'lua> rlua::FromLua<'lua> for PackageReference {
  fn from_lua(
    value: rlua::Value<'lua>,
    _lua_ctx: rlua::Context<'lua>,
  ) -> rlua::Result<PackageReference> {
    // TODO: more validation
    if let rlua::Value::String(s) = value {
      let string = s.to_str()?;
      Ok(PackageReference::new(string).to_lua_err()?)
    } else {
      Err(rlua::Error::external(
        "Expected a string for an object kind",
      ))
    }
  }
}
