use actix::{Actor, StreamHandler, clock::Instant};
use actix_web::{web, App, Error, HttpRequest, HttpResponse, HttpServer, Result, Responder};
use actix_web_actors::ws;
use actix::prelude::*;
use wsserver::DyscyplinerStatus;
use std::time::Duration;

mod wsserver;

/// How often heartbeat pings are sent
const HEARTBEAT_INTERVAL: Duration = Duration::from_secs(5);

/// How long before lack of client response causes a timeout
const CLIENT_TIMEOUT: Duration = Duration::from_secs(10);


struct SiteClient{
    pub id: usize,
    /// Client must send ping at least once per 10 seconds (CLIENT_TIMEOUT),
    /// otherwise we drop connection.
    pub hb: Instant,
    /// Chat server
    pub addr: Addr<wsserver::DyscyplinerServer>,
}

impl SiteClient{
    /// helper method that sends ping to client every 5 seconds (HEARTBEAT_INTERVAL).
    ///
    /// also this method checks heartbeats from client
    fn hb(&self, ctx: &mut ws::WebsocketContext<Self>) {
        ctx.run_interval(HEARTBEAT_INTERVAL, |act, ctx| {
            // check client heartbeats
            if Instant::now().duration_since(act.hb) > CLIENT_TIMEOUT {
                // heartbeat timed out
                println!("Websocket Client heartbeat failed, disconnecting!");

                // notify chat server
                act.addr.do_send(wsserver::Disconnect { id: act.id });

                // stop actor
                ctx.stop();

                // don't try to send a ping
                return;
            }

            ctx.ping(b"");
        });
    }
}


impl Actor for SiteClient {
    type Context = ws::WebsocketContext<Self>;

    fn started(&mut self, ctx: &mut Self::Context) {
        // we'll start heartbeat process on session start.
        //self.hb(ctx);

        // register self in chat server. `AsyncContext::wait` register
        // future within context, but context waits until this future resolves
        // before processing any other events.
        // HttpContext::state() is instance of WsChatSessionState, state is shared
        // across all routes within application
        let addr = ctx.address();

        self.addr
            .send(wsserver::Connect {
                addr: addr.recipient(),
            })
            .into_actor(self)
            .then(|res, act, ctx| {
                match res {
                    Ok(res) => {
                        act.id = res.id;
                        ctx.text(res.init_data);
                    }
                    // something is wrong with chat server
                    _ => ctx.stop(),
                }
                fut::ready(())
            })
            .wait(ctx);

    }
}

/// Handler for ws::Message message
impl StreamHandler<Result<ws::Message, ws::ProtocolError>> for SiteClient {
    fn handle(&mut self, msg: Result<ws::Message, ws::ProtocolError>, ctx: &mut Self::Context) {
        match msg {
            Ok(ws::Message::Ping(msg)) => ctx.pong(&msg),
            Ok(ws::Message::Text(text)) => {
                let m = text.trim();
                let v: Vec<&str> = m.splitn(2, ' ').collect();
                match v[0] {
                    "ADDDEV" =>{
                        println!("Adding device: {}", v[1]);
                        self.addr.do_send(wsserver::AddDevice{name: String::from(v[1])});
                    }
                    _ => {
                        println!("Client sent unknown data: {}", text);
                    }
                }
                //ctx.address().do_send(wsserver::Connect{addr: text.to_string()});
            },
            Ok(ws::Message::Binary(bin)) => ctx.binary(bin),
            _ => (),
        }
    }
}

/// Handle messages from chat server, we simply send it to peer websocket
impl Handler<wsserver::Message> for SiteClient {
    type Result = ();

    fn handle(&mut self, msg: wsserver::Message, ctx: &mut Self::Context) {
        ctx.text(msg.message);
    }
}

async fn index(req: HttpRequest, stream: web::Payload, srv: web::Data<Addr<wsserver::DyscyplinerServer>>, path: web::Path<(String, String)>) -> Result<HttpResponse, Error> {
    let (login, password) = path.into_inner();
    
    if !login.eq("admin") || !password.eq("admin") {
        return Result::Ok(HttpResponse::Unauthorized().finish().map_into_boxed_body());
    }

    let resp = ws::start(SiteClient {id: 0,hb:Instant::now(), addr: srv.get_ref().clone()}, &req, stream);
    println!("{:?}", resp);
    resp
}

async fn device_alive(req: HttpRequest, stream: web::Payload, srv: web::Data<Addr<wsserver::DyscyplinerServer>>, path: web::Path<(String, String)>) -> impl Responder {
    let (key, str_status) = path.into_inner();
    let status = match str_status.as_str() {
        "good" => DyscyplinerStatus::GOOD,
        "angry" => DyscyplinerStatus::ANGRY,
        "dysciplined" => DyscyplinerStatus::DYSCIPLINED,
        _ => DyscyplinerStatus::OFFLINE
    };

    if status == DyscyplinerStatus::OFFLINE {
        return HttpResponse::BadRequest();
    }

    srv.do_send(wsserver::DeviceAlive{key: key, status: status});

    HttpResponse::Ok()
}

#[actix_web::main]
async fn main() -> std::io::Result<()> {
    let server = wsserver::DyscyplinerServer::new().start();

    HttpServer::new(move || 
                App::new()
                .app_data(web::Data::new(server.clone()))
                .route("/ws/{login}/{password}", web::get().to(index))
                .route("/device/alive/{key}/{status}", web::get().to(device_alive)
            )
        )
        .bind(("127.0.0.1", 8080))?
        .run()
        .await
}