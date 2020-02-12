use crate::chat::ToClientMessage;
use crate::lua::{LuaHost, SerializableValue};
use crate::object::actor::{ObjectActorState, ObjectMessage};
use crate::object::api;
use crate::world::{Id, ObjectKind, World, WorldRef};
use rlua;
use std::cell::RefCell;
use std::collections::HashMap;

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
        .context(|lua_ctx| {
          api::register_api(lua_ctx)?;
          lua_host.load_system_package(lua_ctx, "main").map(|_| ()) // drop result so it doesn't outlive closure
        })
        .map(|_| state)
    });

    ObjectExecutor {
      lua_state: ready_state,
      generation: generation,
    }
  }

  pub fn run_for_object<'a, F, T>(
    wf: WorldRef,
    id: Id,
    current_message: &'a ObjectMessage,
    object_state: &'a mut ObjectActorState,
    body: F,
  ) -> rlua::Result<T>
  where
    F: FnOnce(rlua::Context) -> rlua::Result<T>,
  {
    let state = ExecutionState {
      id: id,
      current_message: current_message,
      world: wf.clone(),
      object_state: RefCell::new(object_state),
      writes: RefCell::new(Vec::new()),
      changed_attrs: RefCell::new(HashMap::new()),
    };

    // we hold a read lock on the world as a simple form of "transaction isolation" for now
    // this is not useful right now but prevents us from accidentally writing to the world
    // which could produce globally-visible effects while other objects are running.
    let result = wf.read(|_w| {
      let kind = _w.kind(id);

      _w.with_executor(kind.clone(), |executor| {
        // This is a gross hack but is safe since the scoped thread local ensures
        // this value only exists as long as this block.
        EXECUTION_STATE.set(unsafe { make_static(&state) }, || {
          match &executor.lua_state {
            Ok(lua_state) => lua_state.context(|lua_ctx| body(lua_ctx)),
            Err(e) => {
              log::error!("Lua state failed loading with {:?}; returning failure.", e);
              Err(e.clone())
            }
          }
        })
      })
    });

    let wf = wf.clone();
    wf.write(|w| {
      for write in state.writes.borrow().iter() {
        write.commit(w)
      }
      w.set_attrs(id, state.changed_attrs.borrow().clone())
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
  CreateObject {
    parent: Option<Id>,
    kind: ObjectKind,
    init_message: ObjectMessage,
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
      GlobalWrite::SetLocalPackageContent { kind, content } => {
        world.set_local_package_content(kind.clone(), content.clone())
      }
      GlobalWrite::SendMessage { target, message } => world.send_message(*target, message.clone()),
      GlobalWrite::SendClientMessage { target, message } => {
        world.send_client_message(*target, message.clone())
      }
      GlobalWrite::CreateObject {
        parent,
        kind,
        init_message,
      } => {
        let id = world.create_in(*parent, kind.clone());
        world.send_message(id, init_message.clone())
      }
      GlobalWrite::MoveObject {
        child,
        new_parent,
        original_user,
        sender,
        payload,
      } => {
        world.move_object(*child, *new_parent);
        world.send_message(
          *child,
          ObjectMessage {
            original_user: *original_user,
            immediate_sender: *sender,
            name: "parent_changed".to_string(),
            payload: payload.clone(),
          },
        );

        new_parent.map(|p| {
          world.send_message(
            p,
            ObjectMessage {
              original_user: *original_user,
              immediate_sender: *sender,
              name: "child_added".to_string(),
              payload: payload.clone(),
            },
          );
        });
      }
    }
  }
}

pub(super) struct ExecutionState<'a> {
  pub(super) id: Id,
  pub(super) current_message: &'a ObjectMessage,
  pub(super) world: WorldRef,
  object_state: RefCell<&'a mut ObjectActorState>,
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

  pub(super) fn with_actor_state_mut<T, F>(body: F) -> T
  where
    F: FnOnce(&mut ObjectActorState) -> T,
  {
    Self::with_state(|s| body(&mut s.object_state.borrow_mut()))
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
  current_generation: i64,
  executors: Vec<ObjectExecutor>,
}

impl ExecutorCache {
  pub fn new() -> ExecutorCache {
    ExecutorCache {
      current_generation: 0,
      executors: Vec::new(),
    }
  }

  pub fn update(&mut self) {
    self.current_generation += 1;
    self.executors.clear();
  }

  pub fn checkout_executor(&mut self, lua_host: &LuaHost) -> ObjectExecutor {
    if let Some(executor) = self.executors.pop() {
      executor
    } else {
      ObjectExecutor::new(&lua_host, self.current_generation)
    }
  }

  pub fn checkin_executor(&mut self, executor: ObjectExecutor) {
    if self.current_generation == executor.generation {
      self.executors.push(executor)
    }
  }
}
