mod chat;
mod lua;
mod object;
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

  let res = actix_rt::System::new("main").block_on(run_server());

  info!("Stopping");

  res
}

fn world_load_path() -> PathBuf {
  let state_dir_env = env::var("ORISA_STATE_DIRECTORY").unwrap_or("state".to_string());
  let state_dir = Path::new(&state_dir_env);
  state_dir.join("world.json").to_path_buf()
}

fn load_world(world: &mut World, path: &Path) -> ResultAnyError<()> {
  let file = File::open(path)?;
  world.unfreeze_read(file)?;
  Ok(())
}

fn save_world(world_ref: WorldRef) -> ResultAnyError<()> {
  let state_dir_env = env::var("ORISA_STATE_DIRECTORY").unwrap_or("state".to_string());
  let state_dir = Path::new(&state_dir_env);
  let temp_path = state_dir.join("world-out.json");
  let file = File::create(&temp_path)?;
  World::freeze(world_ref, file)?;

  let final_path = state_dir.join("world.json");
  let _ = copy(final_path.clone(), state_dir.join("world.bak.json")); // ignore result
  rename(temp_path, final_path)?;
  Ok(())
}

async fn run_server() -> Result<(), std::io::Error> {
  let arbiter = Arbiter::new();

  // default to assuming killpop is checked out next to orisa
  let code_dir_env = env::var("ORISA_CODE_DIRECTORY").unwrap_or("../../killpop".to_string());
  let (_world, world_ref) = World::new(arbiter.clone(), &Path::new(&code_dir_env));

  world_ref.write(|w| {
    let path = world_load_path();
    if path.exists() {
      load_world(w, &path).expect("Error loading world")
    } else {
      w.unfreeze_empty(); // start from scratch
    }
  });

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
    let _ =
      save_world(world_ref.clone()).map_err(|err| log::error!("Failed to save world: {}", err));
    executor::block_on(srv.stop(true));
  })
  .expect("Error setting Ctrl-C handler");

  running.await
}
