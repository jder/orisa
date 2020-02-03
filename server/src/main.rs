mod chat;
mod lua;
mod object_actor;
mod util;
mod world;

#[macro_use]
extern crate lazy_static;

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
use std::fs::{copy, rename, File};
use std::path::Path;

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

fn load_world(world: &mut World) -> ResultAnyError<()> {
    let path = Path::new("world.json");
    let file = File::open(&path)?;
    world.unfreeze_read(file)?;
    Ok(())
}

fn save_world(world_ref: WorldRef) -> ResultAnyError<()> {
    let temp_path = Path::new("world-out.json");
    let file = File::create(&temp_path)?;
    World::freeze(world_ref, file).unwrap();

    let final_path = Path::new("world.json");
    let _ = copy(final_path, Path::new("world.bak.json")); // ignore result
    rename(temp_path, final_path)?;
    Ok(())
}

async fn run_server() -> Result<(), std::io::Error> {
    let arbiter = Arbiter::new();

    let (_world, world_ref) = World::new(arbiter.clone(), Path::new("../../killpop/main.lua"));

    world_ref.write(|w| {
        if load_world(w).is_err() {
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
        server.bind("127.0.0.1:8080")?
    };

    info!("Starting!");

    let running = server.run();

    let srv = running.clone();
    ctrlc::set_handler(move || {
        info!("Asking for stop!");
        save_world(world_ref.clone()).unwrap();
        executor::block_on(srv.stop(true));
    })
    .expect("Error setting Ctrl-C handler");

    running.await
}
