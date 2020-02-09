use crate::lua::LuaHost;
use crate::object::actor::ObjectActorState;
use crate::object::api;
use crate::world::{Id, World, WorldRef};
use rlua;
use std::cell::RefCell;

pub struct ObjectExecutor {
  generation: i64,
  // We use a Result here so that if this fails to initialize, it will
  // produce the init error when someone tries to use this executor
  lua_state: rlua::Result<rlua::Lua>,
}

impl ObjectExecutor {
  pub fn new(lua_host: &LuaHost, generation: i64) -> ObjectExecutor {
    let initial_state = lua_host.fresh_state();

    let ready_state: rlua::Result<rlua::Lua> = initial_state.and_then(|state| {
      state
        .context(|lua_ctx| api::register_api(lua_ctx))
        .map(|_| state)
    });

    ObjectExecutor {
      lua_state: ready_state,
      generation: generation,
    }
  }

  pub fn execute<'a, F, T>(
    &self,
    id: Id,
    world: WorldRef,
    object_state: &mut ObjectActorState,
    body: F,
  ) -> rlua::Result<T>
  where
    F: FnOnce(rlua::Context) -> rlua::Result<T>,
  {
    let state = ExecutionState {
      id: id,
      world: world.clone(),
      object_state: RefCell::new(object_state),
    };

    // This is a gross hack but is safe since the scoped thread local ensures
    // this value only exists as long as this block.
    EXECUTION_STATE.set(unsafe { make_static(&state) }, || match &self.lua_state {
      Ok(lua_state) => lua_state.context(|lua_ctx| body(lua_ctx)),
      Err(e) => Err(e.clone()),
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

pub struct ExecutorCache {
  // keep set of executors, plus a version number to discard outdated ones
  lua_host: LuaHost,
  current_generation: i64,
  executors: Vec<ObjectExecutor>,
}

impl ExecutorCache {
  pub fn new(lua_host: &LuaHost) -> ExecutorCache {
    ExecutorCache {
      lua_host: lua_host.clone(),
      current_generation: 0,
      executors: Vec::new(),
    }
  }

  pub fn update(&mut self, lua_host: &LuaHost) {
    self.current_generation += 1;
    self.lua_host = lua_host.clone();
    self.executors.clear();
  }

  pub fn checkout_executor(&mut self) -> ObjectExecutor {
    if let Some(executor) = self.executors.pop() {
      executor
    } else {
      ObjectExecutor::new(&self.lua_host, self.current_generation)
    }
  }

  pub fn checkin_executor(&mut self, executor: ObjectExecutor) {
    if self.current_generation == executor.generation {
      self.executors.push(executor)
    }
  }
}
