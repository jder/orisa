use crate::lua::{LuaHost, PackageReference, SerializableValue};
use crate::object::api;
use crate::object::types::Message;
use crate::world::actor::WorldActor;
use crate::world::state::State as WorldState;
use crate::world::{Id, World, WorldRef};
use rlua;
use std::cell::RefCell;
use std::rc::Rc;

#[derive(Clone)]
pub struct ObjectExecutor {
  world_ref: WorldRef,
  body: Rc<RefCell<ObjectExecutorBody>>,
}

struct ObjectExecutorBody {
  // We use a Result here so that if this fails to initialize, it will
  // produce the init error when someone tries to use this executor
  lua_state: rlua::Result<rlua::Lua>,
}

impl ObjectExecutorBody {
  fn new(lua_state: rlua::Result<rlua::Lua>) -> Rc<RefCell<ObjectExecutorBody>> {
    Rc::new(RefCell::new(ObjectExecutorBody {
      lua_state: lua_state,
    }))
  }
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
      body: ObjectExecutorBody::new(ready_state),
    }
  }

  pub fn run_main<'a>(
    &self,
    actor: &mut WorldActor,
    message: &'a Message,
    is_query: bool,
  ) -> rlua::Result<SerializableValue> {
    self.run_for_object(actor, message, is_query, |lua_ctx| {
      ExecutionState::with_state(|s| s.set_globals(&lua_ctx))?;
      let globals = lua_ctx.globals();
      let main: rlua::Function = globals.get("main")?;
      main.call::<_, SerializableValue>((message.name.clone(), message.payload.clone()))
    })
  }

  pub fn run_for_object<'a, F, T>(
    &self,
    actor: &mut WorldActor,
    current_message: &'a Message,
    is_query: bool,
    body: F,
  ) -> rlua::Result<T>
  where
    F: FnOnce(rlua::Context) -> rlua::Result<T>,
  {
    let state = RefCell::new(ExecutionState {
      current_message: current_message,
      actor: actor,
      world: self.world_ref.clone(),
      in_query: is_query,
      executor: self,
    });

    // This is a gross hack but is safe since the scoped thread local ensures
    // this value only exists as long as this block.
    let wf = self.world_ref.clone();
    EXECUTION_STATE.set(unsafe { make_static(&state) }, || {
      let ObjectExecutorBody {
        lua_state: ref state,
      } = *self.body.borrow();

      match state {
        Ok(lua_state) => lua_state.context(|lua_ctx| {
          let globals = lua_ctx.globals();
          let main: Option<rlua::Function> = globals.get("main")?;
          if main.is_none() {
            // we try loading first so we we re-try on failures to produce the error again
            wf.read(|w| {
              w.get_lua_host()
                .load_filesystem_package(lua_ctx, &PackageReference::main_package())
            })?;
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
  pub(super) actor: &'a mut WorldActor,
  world: WorldRef,
  pub(super) in_query: bool,
  pub(super) executor: &'a ObjectExecutor,
}

impl<'a> ExecutionState<'a> {
  pub(super) fn set_globals(&self, lua_ctx: &rlua::Context) -> rlua::Result<()> {
    let message = self.current_message;
    let globals = lua_ctx.globals();
    let orisa: rlua::Table = globals.get("orisa")?;
    orisa.set("self", message.target)?;
    orisa.set("sender", message.immediate_sender)?;
    orisa.set("original_user", message.original_user)?;
    Ok(())
  }

  pub(super) fn with_state<T, F>(body: F) -> T
  where
    F: FnOnce(&ExecutionState) -> T,
  {
    EXECUTION_STATE.with(|s| body(&s.borrow()))
  }

  pub(super) fn with_state_mut<T, F>(body: F) -> T
  where
    F: FnOnce(&mut ExecutionState) -> T,
  {
    EXECUTION_STATE.with(|s| body(&mut s.borrow_mut()))
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

scoped_thread_local! {static EXECUTION_STATE: RefCell<ExecutionState>}

unsafe fn make_static<'a>(
  p: &'a RefCell<ExecutionState<'a>>,
) -> &'static RefCell<ExecutionState<'static>> {
  use std::mem;
  mem::transmute(p)
}
