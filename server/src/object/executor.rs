use crate::lua::LuaHost;
use crate::object::actor::ObjectActorState;
use crate::object::api;
use crate::world::{Id, World, WorldRef};
use rlua;
use std::cell::RefCell;

pub struct ObjectExecutor {
  lua_state: rlua::Lua,
}

impl ObjectExecutor {
  pub fn new(lua_host: &LuaHost) -> rlua::Result<ObjectExecutor> {
    let result = ObjectExecutor {
      lua_state: lua_host.fresh_state()?,
    };

    result
      .lua_state
      .context(|lua_ctx| api::register_api(lua_ctx))?;

    Ok(result)
  }

  pub fn execute<'a, F, T>(
    &self,
    id: Id,
    world: WorldRef,
    object_state: &mut ObjectActorState,
    body: F,
  ) -> T
  where
    F: FnOnce(rlua::Context) -> T,
  {
    let state = ExecutionState {
      id: id,
      world: world.clone(),
      object_state: RefCell::new(object_state),
    };

    // This is a gross hack but is safe since the scoped thread local ensures
    // this value only exists as long as this block.
    EXECUTION_STATE.set(unsafe { make_static(&state) }, || {
      self.lua_state.context(|lua_ctx| body(lua_ctx))
    })
  }
}

pub(super) struct ExecutionState<'a> {
  id: Id,
  world: WorldRef,
  object_state: RefCell<&'a mut ObjectActorState>,
}

impl<'a> ExecutionState<'a> {
  pub(super) fn with_state<T, F>(body: F) -> T
  where
    F: FnOnce(&ExecutionState) -> T,
  {
    EXECUTION_STATE.with(|s| body(s))
  }

  pub(super) fn with_actor_state_mut<T, F>(body: F) -> T
  where
    F: FnOnce(&mut ObjectActorState) -> T,
  {
    Self::with_state(|s| body(&mut s.object_state.borrow_mut()))
  }

  pub(super) fn with_world<T, F>(body: F) -> T
  where
    F: FnOnce(&World) -> T,
  {
    Self::with_state(|s| s.world.read(|w| body(w)))
  }

  pub(super) fn get_id() -> Id {
    Self::with_state(|s| s.id)
  }
}

scoped_thread_local! {static EXECUTION_STATE: ExecutionState}

unsafe fn make_static<'a>(p: &'a ExecutionState<'a>) -> &'static ExecutionState<'static> {
  use std::mem;
  mem::transmute(p)
}
