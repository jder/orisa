mod chat;
mod object_actor;
mod world;

use crate::chat::{AppState, ChatSocket};
use crate::world::{World, WorldRef};
use actix::Arbiter;
use actix_web::middleware::Logger;
use actix_web::{web, App, HttpRequest, HttpResponse, HttpServer, Responder};
use actix_web_actors::ws;
use listenfd::ListenFd;
use log::info;
use std::fs::File;
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

async fn run_server() -> Result<(), std::io::Error> {
    let arbiter = Arbiter::new();

    let (world, world_ref) = World::new(arbiter.clone());

    let path = Path::new("world.json");
    if let Ok(file) = File::open(&path) {
        world_ref.write(|w| {
            w.load(file).unwrap();
        })
    }

    let data = web::Data::new(AppState {
        world_ref: world_ref,
    });

    let mut listenfd = ListenFd::from_env();

    let mut server = HttpServer::new(move || {
        App::new()
            .app_data(data.clone())
            .wrap(Logger::default())
            .route("/", web::get().to(index))
            .route("/api/socket", web::get().to(socket))
    })
    .shutdown_timeout(1);

    server = if let Some(l) = listenfd.take_tcp_listener(0).unwrap() {
        server.listen(l)?
    } else {
        server.bind("127.0.0.1:8080")?
    };

    info!("Starting!");

    server.run().await
}
