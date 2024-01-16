use std::collections::HashMap;
use std::fmt::Display;
use serde::{Serialize};

use std::time::{Instant, Duration};

use actix::Actor;
use actix::prelude::*;

use rand::distributions::Alphanumeric;
use rand::{Rng, thread_rng};

/// Define HTTP actor
const DEVICE_REFRESH_STATUS_DURATION: Duration = Duration::from_secs(10);
const DEVICE_DEAD_DURATION: Duration = Duration::from_secs(15);


#[derive(Serialize, PartialEq, Clone, Copy)]
pub enum DyscyplinerStatus{
    GOOD, ANGRY, DYSCIPLINED, OFFLINE
}

impl Display for DyscyplinerStatus {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        match *self {
            DyscyplinerStatus::GOOD => write!(f, "GOOD"),
            DyscyplinerStatus::ANGRY => write!(f, "ANGRY"),
            DyscyplinerStatus::DYSCIPLINED => write!(f, "DYSCIPLINED"),
            DyscyplinerStatus::OFFLINE => write!(f, "OFFLINE"),
        }
    }
}

#[derive(Serialize)]
pub struct Dyscypliner {
    id : String, 
    name : String,
    status : DyscyplinerStatus
}

fn generate_random_string(length: usize) -> String {
    let rng = thread_rng();
    let random_chars: Vec<u8> = rng.sample_iter(&Alphanumeric).take(length).collect();
    String::from_utf8(random_chars).expect("strange error")
}

pub struct DyscyplinerServer{
    sessions: Vec<Recipient<Message>>,
    devices : Vec<Dyscypliner>,
    alive_devices: HashMap<String, Instant>
}

impl DyscyplinerServer{
    pub fn new() -> Self{
        DyscyplinerServer{sessions: Vec::new(), devices: Vec::new(), alive_devices: HashMap::new()}
    }

    fn hb(&mut self, ctx: &mut actix::Context<Self>) {
        ctx.run_interval(DEVICE_REFRESH_STATUS_DURATION, |act, ctx| {
            let mut to_disconnect = Vec::new();

            // check client heartbeats
            act.alive_devices.retain(|k,v| {
                    if Instant::now().duration_since(*v) > DEVICE_DEAD_DURATION {
                        to_disconnect.push(k.clone());
                        
                        match act.devices.iter_mut().find(|r| r.id == k.clone()) {
                            Some(device)=>{
                                device.status = DyscyplinerStatus::OFFLINE;
                            }
                            None =>{}
                        }
                        return false;
                    }
                    return true;
                }
            );

            for key in to_disconnect{
                act.send_message(format!("NEWSTATUS {} {}", key, DyscyplinerStatus::OFFLINE).as_str());
            }
        });
    }

    fn send_message(&self, message: &str) {
        for connection in &self.sessions {
            connection.do_send(Message{message: message.to_string()});
        }
    }
}

impl Actor for DyscyplinerServer {
    type Context = actix::Context<Self>;

    fn started(&mut self, ctx: &mut Self::Context) {
        self.hb(ctx);
    }
}

#[derive(Message)]
#[rtype(result = "()")]
pub struct Message{
    pub message : String
}


#[derive(MessageResponse)]
pub struct ConnectResult {
    pub id : usize,
    pub init_data : String
}

#[derive(Message)]
#[rtype(result = "String")]
pub struct AddDevice{
    pub name : String
}


#[derive(Message)]
#[rtype(result = "ConnectResult")]
pub struct Connect{
    pub addr: Recipient<Message>,
}

#[derive(Message)]
#[rtype(result = "()")]
pub struct Disconnect {
    pub id: usize,
}

#[derive(Message)]
#[rtype(result = "()")]
pub struct DeviceAlive {
    pub key: String,
    pub status: DyscyplinerStatus
}

impl Handler<Connect> for DyscyplinerServer {
    type Result = ConnectResult;

    fn handle(&mut self, msg: Connect, _: &mut actix::Context<Self>) -> ConnectResult{
        println!("Someone connect");
        self.sessions.push(msg.addr);

        ConnectResult{id: 0, init_data: format!("INIT {}", serde_json::to_string(&self.devices).expect("Error with serializing with serde init data!"))}
    }
}


impl Handler<Disconnect> for DyscyplinerServer{
    type Result = ();

    fn handle(&mut self, msg: Disconnect, _: &mut actix::Context<Self>) {
        println!("Someone disconnected");

        // let mut rooms: Vec<String> = Vec::new();

        // // remove address
        // if self.sessions.remove(&msg.id).is_some() {
        //     // remove session from all rooms
        //     for (name, sessions) in &mut self.rooms {
        //         if sessions.remove(&msg.id) {
        //             rooms.push(name.to_owned());
        //         }
        //     }
        // }

        // // send message to other users
        // for room in rooms {
        //     self.send_message(&room, "Someone disconnected", 0);
        // }
    }
}

impl Handler<AddDevice> for DyscyplinerServer{
    type Result = String;

    fn handle(&mut self, msg: AddDevice, _: &mut actix::Context<Self>) -> String {
        let id = generate_random_string(16);
        //let id = String::from("bob");
        let device = Dyscypliner{id: id.clone(), name: msg.name.clone(), status: DyscyplinerStatus::OFFLINE };

        self.devices.push(device);
        println!("Device added");
        self.send_message(format!("NEWDEV {} {} {}",id, msg.name, DyscyplinerStatus::OFFLINE).as_str());

        
        
        id
    }
}

impl Handler<DeviceAlive> for DyscyplinerServer{
    type Result = ();

    fn handle(&mut self, msg: DeviceAlive, _: &mut actix::Context<Self>) -> () {

        let mut send_message = false;
        let mut add_to_alive = false;

        match self.alive_devices.get_mut(&msg.key.clone()){
            Some(timestamp) => {
                *timestamp = Instant::now();

                match self.devices.iter_mut().find(|r| r.id == msg.key.clone()) {
                    Some(device)=>{
                        if device.status != msg.status {
                            device.status = msg.status;
                            send_message = true;
                        }
                    }
                    None =>{}
                }
            }
            None => {
                match self.devices.iter_mut().find(|r| r.id == msg.key.clone()) {
                    Some(device)=>{
                        device.status = msg.status;
                        add_to_alive = true;
                        send_message = true;
                    }
                    None => {}
                }
            }   
        };

        if add_to_alive {
            self.alive_devices.insert(msg.key.clone(), Instant::now());
        }

        if send_message {
            self.send_message(format!("NEWSTATUS {} {}", msg.key, msg.status).as_str());
        }   
    }
}

