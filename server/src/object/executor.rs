use crate::lua::LuaHost;
use crate::lua::PackageReference;
use crate::object::api;
use crate::object::types::Message;
use crate::world::state::State as WorldState;
use crate::world::{Id, World, WorldRef};
use rlua;

pub struct ObjectExecutor {
  world_ref: WorldRef,
  // We use a Result here so that if this fails to initialize, it will
  // produce the init error when someone tries to use this executor
  lua_state: rlua::Result<rlua::Lua>,
  loaded_main: bool,
}

impl ObjectExecutor {
  pub fn new(lua_host: &LuaHost, world_ref: WorldRef) -> ObjectExecutor {
    let initial_state = lua_host.fresh_state();

    let ready_state: rlua::Result<rlua::Lua> = initial_state.and_then(|state| {
      state
        .context(|lua_ctx| api::register_api(lua_ctx))
        .map(|_| state)
    });

    ObjectExecutor {
      world_ref: world_ref,
      lua_state: ready_state,
      loaded_main: false,
    }
  }

  pub fn run_for_object<'a, F, T>(
    &mut self,
    current_message: &'a Message,
    body: F,
  ) -> rlua::Result<T>
  where
    F: FnOnce(rlua::Context) -> rlua::Result<T>,
  {
    let state = ExecutionState {
      current_message: current_message,
      world: self.world_ref.clone(),
      in_query: false,
    };

    // This is a gross hack but is safe since the scoped thread local ensures
    // this value only exists as long as this block.
    let wf = self.world_ref.clone();
    EXECUTION_STATE.set(unsafe { make_static(&state) }, || {
      let (state, loaded_main) = (&self.lua_state, &mut self.loaded_main);

      match state {
        Ok(lua_state) => lua_state.context(|lua_ctx| {
          if !*loaded_main {
            // we try loading first so we we re-try on failures to produce the error again
            wf.read(|w| {
              w.get_lua_host()
                .load_filesystem_package(lua_ctx, &PackageReference::main_package())
            })?;
            *loaded_main = true;
          }
          body(lua_ctx)
        }),
        Err(e) => {
          log::error!("Lua state failed loading with {:?}; returning failure.", e);
          Err(e.clone())
        }
      }
    })
  }
}

pub(super) struct ExecutionState<'a> {
  pub(super) current_message: &'a Message,
  world: WorldRef,
  in_query: bool,
}

impl<'a> ExecutionState<'a> {
  pub(super) fn with_state<T, F>(body: F) -> T
  where
    F: FnOnce(&ExecutionState) -> T,
  {
    EXECUTION_STATE.with(|s| body(s))
  }

  pub(super) fn with_world<T, F>(body: F) -> T
  where
    F: FnOnce(&World) -> T,
  {
    Self::with_state(|s| s.world.read(|w| body(w)))
  }

  pub(super) fn with_world_mut<T, F>(body: F) -> rlua::Result<T>
  where
    F: FnOnce(&mut World) -> rlua::Result<T>,
  {
    Self::with_state(|s| {
      if s.in_query {
        Err(rlua::Error::external("Unable to set/send during a query."))
      } else {
        s.world.write(|w| body(w))
      }
    })
  }

  pub(super) fn with_world_state<T, F>(body: F) -> T
  where
    F: FnOnce(&WorldState) -> T,
  {
    Self::with_state(|s| s.world.read(|w| body(w.get_state())))
  }

  pub(super) fn with_world_state_mut<T, F>(body: F) -> rlua::Result<T>
  where
    F: FnOnce(&mut WorldState) -> rlua::Result<T>,
  {
    Self::with_state(|s| {
      if s.in_query {
        Err(rlua::Error::external("Unable to set/send during a query."))
      } else {
        s.world.write(|w| body(w.get_state_mut()))
      }
    })
  }

  pub(super) fn get_id() -> Id {
    Self::with_state(|s| s.current_message.target)
  }

  pub(super) fn get_original_user() -> Option<Id> {
    Self::with_state(|s| s.current_message.original_user)
  }
}

scoped_thread_local! {static EXECUTION_STATE: ExecutionState}

unsafe fn make_static<'a>(p: &'a ExecutionState<'a>) -> &'static ExecutionState<'static> {
  use std::mem;
  mem::transmute(p)
}
