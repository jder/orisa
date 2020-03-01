mod chat;
mod lua;
mod object;
mod repo;
mod util;
mod world;

use crate::chat::{AppState, ChatSocket};
use crate::util::ResultAnyError;
use crate::world::{World, WorldRef};
use actix::Arbiter;
use actix_web::middleware::Logger;
use actix_web::{web, App, HttpRequest, HttpResponse, HttpServer, Responder};
use actix_web_actors::ws;
use futures::executor;
use listenfd::ListenFd;
use log::info;
use std::env;
use std::fs::{copy, rename, File};
use std::path::{Path, PathBuf};
use std::sync::{Arc, RwLock};

#[macro_use]
extern crate scoped_tls;

#[macro_use]
extern crate lazy_static;

async fn index() -> impl Responder {
  HttpResponse::Ok().body("Hello world!")
}

async fn socket(
  req: HttpRequest,
  stream: web::Payload,
  data: web::Data<AppState>,
) -> impl Responder {
  let res = ws::start(ChatSocket::new(data.clone()), &req, stream);
  res
}

fn main() -> Result<(), std::io::Error> {
  env_logger::init();

  let mut system = actix_rt::System::builder()
    .name("main")
    .stop_on_panic(true)
    .build();

  // This reference to _world keeps it alive
  let (_world, world_ref) = build_world()?;

  let res = system.block_on(run_server(world_ref.clone()));

  info!("Saving world before stop");

  save_world(world_ref.clone()).expect("Failed to save world");

  info!("Save complete");

  res
}

fn world_load_path() -> PathBuf {
  let state_dir_env = env::var("ORISA_STATE_DIRECTORY").unwrap_or("state".to_string());
  let state_dir = Path::new(&state_dir_env);
  state_dir.join("world.json").to_path_buf()
}

fn save_world(world_ref: WorldRef) -> ResultAnyError<()> {
  let state_dir_env = env::var("ORISA_STATE_DIRECTORY").unwrap_or("state".to_string());
  let state_dir = Path::new(&state_dir_env);
  let temp_path = state_dir.join("world-out.json");
  let file = File::create(&temp_path)?;
  world_ref.read(|w| w.save(file))?;

  let final_path = state_dir.join("world.json");
  let _ = copy(final_path.clone(), state_dir.join("world.bak.json")); // ignore result
  rename(temp_path, final_path)?;
  Ok(())
}

fn build_world() -> Result<(Arc<RwLock<Option<World>>>, WorldRef), std::io::Error> {
  let arbiter = Arbiter::new();

  // default to assuming killpop is checked out next to orisa
  let code_dir_env = env::var("ORISA_CODE_DIRECTORY").unwrap_or("../../killpop".to_string());
  log::info!("Using code directory {}", code_dir_env);

  let code_remote = env::var("ORISA_CODE_REMOTE").ok();
  let code_branch = env::var("ORISA_CODE_BRANCH").ok();

  let git_config = match (code_remote, code_branch) {
    (Some(remote), Some(branch)) => {
      log::info!(
        "Configured to pull system code from {} -> {}",
        remote,
        branch
      );
      Some(repo::Repo::new(&Path::new(&code_dir_env), remote, branch))
    }
    _ => {
      log::info!("Not configured to pull system code");
      None
    }
  };

  let path = world_load_path();
  let read = if path.exists() {
    Some(File::open(path).expect("Error opening world"))
  } else {
    None
  };

  Ok(
    World::new(&arbiter, &Path::new(&code_dir_env), git_config, read).expect("error loading world"),
  )
}

async fn run_server(world_ref: WorldRef) -> Result<(), std::io::Error> {
  let data = web::Data::new(AppState {
    world_ref: world_ref.clone(),
  });

  let mut listenfd = ListenFd::from_env();

  let mut server = HttpServer::new(move || {
    App::new()
      .app_data(data.clone())
      .wrap(Logger::default())
      .route("/", web::get().to(index))
      .route("/api/socket", web::get().to(socket))
  })
  .shutdown_timeout(1)
  .disable_signals();

  server = if let Some(l) = listenfd.take_tcp_listener(0).unwrap() {
    server.listen(l)?
  } else {
    server.bind("0.0.0.0:8080")?
  };

  info!("Starting!");

  let running = server.run();

  let srv = running.clone();
  ctrlc::set_handler(move || {
    info!("Asking for stop!");
    executor::block_on(srv.stop(true));
  })
  .expect("Error setting Ctrl-C handler");

  running.await
}
