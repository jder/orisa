use crate::chat::ToClientMessage;
use crate::lua::PackageReference;
use crate::lua::{LuaHost, SerializableValue};
use crate::object::api;
use crate::object::types::Message;
use crate::world::state::State as WorldState;
use crate::world::{Id, World, WorldRef};
use rlua;
use std::cell::RefCell;
use std::collections::HashMap;

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
      writes: RefCell::new(Vec::new()),
      changed_attrs: RefCell::new(HashMap::new()),
    };

    // This is a gross hack but is safe since the scoped thread local ensures
    // this value only exists as long as this block.
    let wf = self.world_ref.clone();
    let result = EXECUTION_STATE.set(unsafe { make_static(&state) }, || {
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
    });

    wf.write(|w| {
      for write in state.writes.borrow().iter() {
        write.commit(w)
      }
      w.get_state_mut()
        .set_attrs(current_message.target, state.changed_attrs.borrow().clone())
    });
    result
  }
}

// For things we collect during execution but only commit afterwards
// until we have a real transaction isolation story.
// These must be semantically commutative with any reads and writes other
// objects might do. Note that this means that attribute writes act
// potentially async. (Since another object can read & commit between
// your reads and commits.)
// For example, if you read your name and another object's name, and change
// yours if they are the same, another object might also see the same
// thing and you could both end up changing names.
#[derive(Clone)]
pub enum GlobalWrite {
  SetLocalPackageContent {
    package: PackageReference,
    content: String,
  },
  SendMessage {
    message: Message,
  },
  SendClientMessage {
    target: Id,
    message: ToClientMessage,
  },
  InitializeObject {
    id: Id,
    parent: Option<Id>,
    init_message: Message,
  },
  MoveObject {
    child: Id,
    new_parent: Option<Id>,
    original_user: Option<Id>,
    sender: Id,
    payload: SerializableValue,
  },
}

impl GlobalWrite {
  // TODO: would be nice to consume here but it's tricky due to the schenanigans above
  pub fn commit(&self, world: &mut World) {
    match self {
      GlobalWrite::SetLocalPackageContent { package, content } => {
        world
          .get_state_mut()
          .set_live_package_content(package.clone(), content.clone());
        world.reload_code(); // TODO: restrict to just this kind
      }
      GlobalWrite::SendMessage { message } => world.send_message(message.clone()),
      GlobalWrite::SendClientMessage { target, message } => {
        world.send_client_message(*target, message.clone())
      }
      GlobalWrite::InitializeObject {
        id,
        parent,
        init_message,
      } => {
        world.get_state_mut().move_object(*id, *parent);
        world.send_message(init_message.clone())
      }
      GlobalWrite::MoveObject {
        child,
        new_parent,
        original_user,
        sender,
        payload,
      } => {
        world.get_state_mut().move_object(*child, *new_parent);
        world.send_message(Message {
          target: *child,
          original_user: *original_user,
          immediate_sender: *sender,
          name: "parent_changed".to_string(),
          payload: payload.clone(),
        });

        new_parent.map(|p| {
          world.send_message(Message {
            target: p,
            original_user: *original_user,
            immediate_sender: *sender,
            name: "child_added".to_string(),
            payload: payload.clone(),
          });
        });
      }
    }
  }
}

pub(super) struct ExecutionState<'a> {
  pub(super) current_message: &'a Message,
  pub(super) world: WorldRef,
  writes: RefCell<Vec<GlobalWrite>>,
  changed_attrs: RefCell<HashMap<String, SerializableValue>>,
}

impl<'a> ExecutionState<'a> {
  pub(super) fn with_state<T, F>(body: F) -> T
  where
    F: FnOnce(&ExecutionState) -> T,
  {
    EXECUTION_STATE.with(|s| body(s))
  }

  pub(super) fn add_write(write: GlobalWrite) {
    Self::with_state(|s| s.writes.borrow_mut().push(write))
  }

  pub(super) fn with_changed_attrs<T, F>(body: F) -> T
  where
    F: FnOnce(&mut HashMap<String, SerializableValue>) -> T,
  {
    Self::with_state(|s| body(&mut s.changed_attrs.borrow_mut()))
  }

  pub(super) fn with_world<T, F>(body: F) -> T
  where
    F: FnOnce(&World) -> T,
  {
    Self::with_state(|s| s.world.read(|w| body(w)))
  }

  pub(super) fn with_world_mut<T, F>(body: F) -> T
  where
    F: FnOnce(&mut World) -> T,
  {
    Self::with_state(|s| s.world.write(|w| body(w)))
  }

  pub(super) fn with_world_state<T, F>(body: F) -> T
  where
    F: FnOnce(&WorldState) -> T,
  {
    Self::with_state(|s| s.world.read(|w| body(w.get_state())))
  }

  pub(super) fn with_world_state_mut<T, F>(body: F) -> T
  where
    F: FnOnce(&mut WorldState) -> T,
  {
    Self::with_state(|s| s.world.write(|w| body(w.get_state_mut())))
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
