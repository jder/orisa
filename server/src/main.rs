use actix::Arbiter;
use actix_web::middleware::Logger;
use actix_web::{web, App, HttpRequest, HttpResponse, HttpServer, Responder};

use actix_web_actors::ws;
use listenfd::ListenFd;
use log::info;

mod object_actor;
mod world;
use crate::world::World;

mod chat;
use crate::chat::{AppState, ChatSocket};

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

#[actix_rt::main]
async fn main() -> std::io::Result<()> {
    env_logger::init();

    let arbiter = Arbiter::new();

    let (world, world_ref) = World::new(arbiter.clone());

    let data = web::Data::new(AppState {
        world: world,
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
    .shutdown_timeout(5);

    server = if let Some(l) = listenfd.take_tcp_listener(0).unwrap() {
        server.listen(l)?
    } else {
        server.bind("127.0.0.1:8080")?
    };

    info!("Starting!");

    server.run().await
}
