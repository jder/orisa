use crate::chat::ToClientMessage;
use crate::lua::LuaHost;
use crate::object::actor::ObjectActorState;
use crate::object::actor::ObjectMessage;
use crate::object::api;
use crate::world::{Id, ObjectKind, World, WorldRef};
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
    current_message: &'a ObjectMessage,
    world: WorldRef,
    object_state: &'a mut ObjectActorState,
    body: F,
  ) -> rlua::Result<(T, Vec<GlobalWrite>)>
  // TODO: would be nice to not have writes cancel if there's an error
  // but we have some typing issues with this + world executor interface
  where
    F: FnOnce(rlua::Context) -> rlua::Result<T>,
  {
    let state = ExecutionState {
      id: id,
      current_message: current_message,
      world: world.clone(),
      object_state: RefCell::new(object_state),
      writes: RefCell::new(Vec::new()),
    };

    // This is a gross hack but is safe since the scoped thread local ensures
    // this value only exists as long as this block.
    let result = EXECUTION_STATE.set(unsafe { make_static(&state) }, || match &self.lua_state {
      Ok(lua_state) => lua_state.context(|lua_ctx| body(lua_ctx)),
      Err(e) => Err(e.clone()),
    });
    result.map(|t| {
      let writes = state.writes.borrow().clone();
      (t, writes)
    })
  }
}

// For things we collect during execution but only commit afterwards
// until we have a real transaction isolation story.
#[derive(Clone)]
pub enum GlobalWrite {
  SetCustomSpaceContent {
    kind: ObjectKind,
    content: String,
  },
  SendMessage {
    target: Id,
    message: ObjectMessage,
  },
  SendClientMessage {
    target: Id,
    message: ToClientMessage,
  },
}

impl GlobalWrite {
  // TODO: would be nice to consume here but it's tricky due to the schenanigans above
  pub fn commit(&self, world: &mut World) {
    match self {
      GlobalWrite::SetCustomSpaceContent { kind, content } => {
        world.set_custom_space_content(kind.clone(), content.clone())
      }
      GlobalWrite::SendMessage { target, message } => world.send_message(*target, message.clone()),
      GlobalWrite::SendClientMessage { target, message } => {
        world.send_client_message(*target, message.clone())
      }
    }
  }
}

pub(super) struct ExecutionState<'a> {
  id: Id,
  current_message: &'a ObjectMessage,
  world: WorldRef,
  object_state: RefCell<&'a mut ObjectActorState>,
  writes: RefCell<Vec<GlobalWrite>>,
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

  pub(super) fn add_write(write: GlobalWrite) {
    Self::with_state(|s| s.writes.borrow_mut().push(write))
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

  pub(super) fn get_original_user() -> Option<Id> {
    Self::with_state(|s| s.current_message.original_user)
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
  pub fn new(lua_host: LuaHost) -> ExecutorCache {
    ExecutorCache {
      lua_host: lua_host,
      current_generation: 0,
      executors: Vec::new(),
    }
  }

  pub fn update(&mut self, lua_host: LuaHost) {
    self.current_generation += 1;
    self.lua_host = lua_host;
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
