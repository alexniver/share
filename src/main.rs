//! Run with
//!
//!
//! ```not_rust
//! cargo run -p file-transfer
//! ```
//!
use std::{
    net::SocketAddr,
    path::PathBuf,
    sync::{Arc, Mutex},
    time::{Duration, SystemTime, UNIX_EPOCH},
};

use axum::{
    body::{self, Full},
    extract::{
        ws::{Message, WebSocket},
        Path, State, WebSocketUpgrade,
    },
    http::{header, HeaderValue, Response, StatusCode},
    response::IntoResponse,
    routing::get,
    Router,
};
use bytes::{Buf, BufMut, Bytes, BytesMut};
use clap::{arg, command, Parser};
use futures::{SinkExt, StreamExt};
use home::home_dir;
use tokio::{
    fs::File,
    io::{BufReader, BufWriter},
    sync::broadcast,
};
use tower_http::services::ServeDir;
use tracing::debug;
use tracing_subscriber::{prelude::__tracing_subscriber_SubscriberExt, util::SubscriberInitExt};

// const UPLOAD_DIR: &str = "upload";

const CLIENT_M_ALL_MSG: u8 = 1;
const CLIENT_M_SINGLE_MSG: u8 = 2;
const CLIENT_M_SEND_MSG: u8 = 3;
const CLIENT_SEND_FILE: u8 = 4;

const SERVER_M_ALL_MSG: u8 = 61;
const SERVER_M_CREATE_MSG: u8 = 62;
const SERVER_M_DELETE_MSG: u8 = 63;

/// Simple program to greet a person
#[derive(Parser, Debug)]
#[command(author, version, about, long_about = None)]
struct Args {
    /// Number of times to greet
    #[arg(short, long, default_value_t = 3000)]
    port: u16,
}

fn share_path() -> PathBuf {
    home::home_dir().expect("can't get home dir").join("Share")
}

#[tokio::main]
async fn main() {
    tracing_subscriber::registry()
        .with(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| "ruler=trace".into()),
        )
        .with(tracing_subscriber::fmt::layer())
        .init();

    let args = Args::parse();

    //let _ = tokio::fs::remove_dir_all(UPLOAD_DIR).await; // clear all data
    let share_path = share_path();
    let _ = std::fs::create_dir_all(&share_path);
    let dir = std::fs::read_dir(&share_path).expect("read share dir fail");

    let mut file_name_list = vec![];
    for entry in dir {
        let entry = entry.expect("share entry fail");
        let path = entry.path();
        if path.is_file() {
            file_name_list.push(String::from(
                path.file_name()
                    .expect("fail name error")
                    .to_str()
                    .expect("fail"),
            ));
        }
    }

    let state = Arc::new(AppState::new(file_name_list));

    // state.files.lock().unwrap().push(value);

    let app = Router::new()
        .nest_service("/", ServeDir::new("frontend/build/"))
        .route("/ws", get(websocket_handler))
        .route("/queryfile/*path", get(query_file))
        .with_state(state);

    let addr = SocketAddr::from(([0, 0, 0, 0], args.port));
    debug!("listening on {:?}", &addr);

    axum::Server::bind(&addr)
        .serve(app.into_make_service())
        .await
        .unwrap();
}

async fn websocket_handler(
    wsu: WebSocketUpgrade,
    State(state): State<Arc<AppState>>,
) -> impl IntoResponse {
    wsu.on_upgrade(|ws| websocket(ws, state))
}

async fn websocket(ws: WebSocket, state: Arc<AppState>) {
    let (mut sender, mut reader) = ws.split();

    let state_for_send = state.clone();
    let state_for_delete = state.clone();
    let mut rx = state.tx.subscribe();
    let tx_for_create = state.tx.clone();
    let tx_for_delete = state.tx.clone();

    // send all msg
    let mut bts = BytesMut::new();
    bts.put_u8(SERVER_M_ALL_MSG);
    for msg in state.msg_arr.lock().unwrap().iter() {
        bts.put_i32_le(4);
        bts.put_i32_le(msg.id);

        bts.put_i32_le(4);
        bts.put_i32_le(msg.msg_type);

        let text_byte_arr = msg.text.as_bytes();
        bts.put_i32_le(text_byte_arr.len() as i32);
        bts.put(text_byte_arr);
    }
    let _ = sender.send(Message::Binary(bts.to_vec())).await;

    let mut recv = tokio::spawn(async move {
        while let Some(Ok(data)) = reader.next().await {
            if let Message::Binary(data) = data {
                let mut bts = Bytes::from(data);

                let method_u8 = bts.get_u8();
                // debug!("method: {:?}", method_u8);

                match method_u8 {
                    // query all
                    CLIENT_M_ALL_MSG => {
                        // TOOD;
                    }
                    // query single
                    CLIENT_M_SINGLE_MSG => {
                        let id = bts.get_i32_le();
                        let _ = tx_for_create.send(Crud::Create(id));
                    }
                    // client send msg
                    CLIENT_M_SEND_MSG => {
                        if state.msg_arr.lock().unwrap().len() > 20 {
                            continue;
                        }
                        let msg_len = bts.get_i32_le() as usize;
                        let buf = bts.take(msg_len).into_inner().to_vec();
                        let msg = String::from_utf8(buf).unwrap();
                        let id = state.next_id();
                        // debug!("id: {:?}, msg: {:?}", id, msg);
                        state
                            .msg_arr
                            .lock()
                            .unwrap()
                            .push(Msg::new(id, MSG_T_TEXT, msg));
                        let _ = tx_for_create.send(Crud::Create(id));
                    }
                    // client send file
                    CLIENT_SEND_FILE => {
                        if state.msg_arr.lock().unwrap().len() > 20 {
                            continue;
                        }
                        // 2: upload file, totallen-len-filename-len-filedata
                        let name_len = bts.get_i32_le() as usize;

                        let name = String::from_utf8(bts.split_to(name_len).to_vec()).unwrap();

                        let data_len = bts.get_i32_le() as usize;
                        if data_len > 50 * 1024 * 1024 {
                            continue;
                        }
                        let path = share_path().join(&name);
                        let mut file = BufWriter::new(File::create(path).await.unwrap());
                        let v = bts.take(data_len).into_inner().to_vec();
                        let mut data_reader = BufReader::new(v.as_slice());

                        // Copy the body into the file.
                        tokio::io::copy(&mut data_reader, &mut file).await.unwrap();

                        let id = state.next_id();
                        // debug!("id: {:?}, msg: {:?}", id, msg);
                        state
                            .msg_arr
                            .lock()
                            .unwrap()
                            .push(Msg::new(id, MSG_T_FILE, name));
                        let _ = tx_for_create.send(Crud::Create(id));
                    }
                    _ => {}
                }
            }
        }
    });

    let mut send = tokio::spawn(async move {
        while let Ok(id) = rx.recv().await {
            match id {
                Crud::Create(id) => {
                    let mut bts = BytesMut::new();
                    bts.put_u8(SERVER_M_CREATE_MSG);
                    if let Some(msg) = state_for_send
                        .msg_arr
                        .lock()
                        .unwrap()
                        .iter()
                        .find(|m| m.id == id)
                    {
                        // debug!("single, id: {id}");
                        bts.put_i32_le(4);
                        bts.put_i32_le(msg.id);

                        bts.put_i32_le(4);
                        bts.put_i32_le(msg.msg_type);

                        let text_byte_arr = msg.text.as_bytes();
                        bts.put_i32_le(text_byte_arr.len() as i32);
                        bts.put(text_byte_arr);
                    }
                    if !bts.is_empty() {
                        let _ = sender.send(Message::Binary(bts.to_vec())).await;
                    }
                }
                Crud::Delete(id) => {
                    let mut bts = BytesMut::new();
                    bts.put_u8(SERVER_M_DELETE_MSG);
                    bts.put_i32_le(4);
                    bts.put_i32_le(id);
                    let _ = sender.send(Message::Binary(bts.to_vec())).await;
                }
            }
        }
    });

    // delete timout msg task
    // let mut auto_delete_msg = tokio::spawn(async move {
    //     loop {
    //         tokio::time::sleep(Duration::from_millis(1000)).await;
    //
    //         let mut id_to_remove = vec![];
    //         for msg in state_for_delete.msg_arr.lock().unwrap().iter() {
    //             let now = SystemTime::now()
    //                 .duration_since(UNIX_EPOCH)
    //                 .expect("time error")
    //                 .as_millis();
    //             if now - msg.create_time > 1000 * 60 {
    //                 id_to_remove.push(msg.id);
    //                 let _ = tx_for_delete.send(Crud::Delete(msg.id));
    //             }
    //         }
    //
    //         for id in id_to_remove {
    //             let mut path = None;
    //
    //             {
    //                 let mut msg_arr = state_for_delete.msg_arr.lock().unwrap();
    //                 let pos = msg_arr.iter().position(|msg| msg.id == id);
    //                 if let Some(pos) = pos {
    //                     let msg = msg_arr.remove(pos);
    //                     if msg.msg_type == MSG_T_FILE {
    //                         path = Some(std::path::Path::new(UPLOAD_DIR).join(&msg.text));
    //                     }
    //                 }
    //             }
    //
    //             if let Some(path) = path {
    //                 let _ = tokio::fs::remove_file(path).await;
    //             }
    //         }
    //     }
    // });

    tokio::select! {
        _ = (&mut recv) => {
            send.abort();
            // auto_delete_msg.abort();
        },
        _ = (&mut send) => {
            recv.abort();
            // auto_delete_msg.abort();
        },
        // _ = (&mut auto_delete_msg) => {
        //     send.abort();
        //     recv.abort();
        // },
    }
}

async fn query_file(Path(path): Path<String>) -> impl IntoResponse {
    let path = path.trim_start_matches('/');
    let mime_type = mime_guess::from_path(path).first_or_text_plain();

    let path = share_path().join(path);
    if let Ok(file) = tokio::fs::read(path).await {
        Response::builder()
            .status(StatusCode::OK)
            .header(
                header::CONTENT_TYPE,
                HeaderValue::from_str(mime_type.as_ref()).unwrap(),
            )
            .body(body::boxed(Full::from(file)))
            .unwrap()
    } else {
        Response::builder()
            .status(StatusCode::NO_CONTENT)
            .header(header::CONTENT_TYPE, "")
            .body(body::boxed(Full::default()))
            .unwrap()
    }
}

struct AppState {
    tx: broadcast::Sender<Crud>,
    id_gen: Mutex<i32>,
    msg_arr: Mutex<Vec<Msg>>,
}

impl AppState {
    fn new(file_name_list: Vec<String>) -> Self {
        let (tx, _) = broadcast::channel(128);
        let mut id = 1;
        let mut msg_list = vec![];
        for file_name in file_name_list {
            msg_list.push(Msg::new(id, MSG_T_FILE, file_name));
            id += 1;
        }
        AppState {
            tx,
            id_gen: Mutex::new(id),
            msg_arr: Mutex::new(msg_list),
        }
    }

    fn next_id(&self) -> i32 {
        let result = *self.id_gen.lock().unwrap();
        *self.id_gen.lock().unwrap() += 1;
        result
    }
}

type MsgType = i32;
const MSG_T_TEXT: MsgType = 1;
const MSG_T_FILE: MsgType = 2;

struct Msg {
    id: i32,
    msg_type: MsgType,
    text: String,
    create_time: u128,
}

impl Msg {
    fn new(id: i32, msg_type: MsgType, text: String) -> Self {
        Self {
            id,
            msg_type,
            text,
            create_time: SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .expect("error time")
                .as_millis(),
        }
    }
}

#[derive(Clone, Copy)]
enum Crud {
    Create(i32),
    Delete(i32),
}
