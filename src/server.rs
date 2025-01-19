use futures::StreamExt;
use hyper::{self as http, body::Bytes, header::{CONTENT_LENGTH, CONTENT_TYPE}, service::{make_service_fn, service_fn}, Body, HeaderMap, Request, Response};
use lazy_static_include::lazy_static_include_bytes;
use multipart_stream::Part;
use tokio::{sync::{broadcast, watch}, time::interval};
use tokio_stream::wrappers::BroadcastStream;
use std::{collections::HashMap, sync::{Arc, Mutex}, time::Duration};
use serde::{Deserialize, Serialize};


lazy_static_include_bytes! {
    DEFAULT_BODY_CONTENT => "res/no_feed_640x480.jpg",
}

const BOUNDARY_STRING :&str = "-----mjpeg-proxy-boundary-----";

const fn _none<T>() -> Option<T> {
    None
}

const fn _default_port() -> u16 {
    8080
}

fn _localhost() -> String {
    "127.0.0.1".to_string()
}


async fn serve(req: Request<Body>, config: HashMap<String, Arc<Mutex<broadcast::Sender<Part>>>>) -> Result<Response<Body>, Box<dyn std::error::Error + Send + Sync>> {
    let path = req.uri().to_string();
    println!("Accessed: {}", path);
    let res = config.get(&path);
    if let Some(r) = res {
        let recv = r.lock().unwrap().subscribe();
        let stream = BroadcastStream::new(recv);
        let stream = multipart_stream::serialize(stream, BOUNDARY_STRING);
        Ok(hyper::Response::builder()
            .header(http::header::CONTENT_TYPE, "multipart/mixed; boundary=".to_string() + BOUNDARY_STRING)
            .body(hyper::Body::wrap_stream(stream))?)
    } else {
        println!("404 returned");
        Ok(hyper::Response::builder()
            .status(404)
            .body(hyper::Body::from("404 Not found"))?
        )
    }

}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct Stream {
    pub fps: u8,
    pub url: String,
    pub path: String,
    #[serde(skip, default = "_none::<Arc<Mutex<broadcast::Sender<Part>>>>")]
    pub broadcaster: Option<Arc<Mutex<broadcast::Sender<Part>>>>,
}

#[derive(Serialize, Deserialize, Debug)]
pub struct Server {
    #[serde(default = "_default_port")]
    pub port: u16,
    #[serde(default = "_localhost")]
    pub address: String,
    pub streams: HashMap<String, Stream>,
}

impl Server {
    pub fn new(config: Server) -> Self {
        Server { streams: config.streams, port: config.port, address: config.address }
    }

    pub async fn run(&self) {
        let addr = format!("{}:{}", &self.address, self.port).parse().unwrap();
        let mut handles = Vec::new();
        let mut stream_config = HashMap::new();
        let mut config = HashMap::new();
        for v in self.streams.iter() {
            stream_config.insert(v.1.path.clone(), Arc::clone(v.1.broadcaster.as_ref().unwrap()));
            config.insert(v.1.path.clone(), v.1.clone());
        }

        let make_svc = make_service_fn({
            move |_conn| {
                futures::future::ok::<_, std::convert::Infallible>(
                    service_fn({
                        let v1 = stream_config.clone();
                        move |req| {
                            let v2 = v1.clone();
                            serve(req, v2)
                        }
                    })
                )
            }
        });

        tokio::spawn(async move {
            let server = hyper::Server::bind(&addr).serve(make_svc);
            println!("Serving on http://{}", server.local_addr());
            server.await.unwrap()
        });

        for stream in config.into_iter() {
            let default_body = Bytes::from(&DEFAULT_BODY_CONTENT[..]);
            let mut headers = HeaderMap::new();
            headers.insert(CONTENT_TYPE, "image/jpeg".parse().unwrap());
            headers.insert(CONTENT_LENGTH, "8857".parse().unwrap());
            let default_part = Part{headers, body: default_body};
            println!("{} {} {}", stream.1.path, stream.1.url, stream.1.fps);

            // start server broadcast channel writer
            let (tx, mut rx) = watch::channel(default_part.clone());
            tokio::spawn(async move {
                // Use the equivalent of a "do-while" loop so the initial value is
                // processed before awaiting the `changed()` future.
                let mut interval = interval(Duration::from_micros(1000000 / stream.1.fps as u64));
                loop {
                    tokio::select!(
                        _ = interval.tick() => {
                            let latest = rx.borrow().clone();
                            //println!("recv: {:?}", latest.headers);
                            let bc = stream.1.broadcaster.as_ref().unwrap();
                            let btx = bc.lock().unwrap();
                            let _res = btx.send(latest);
                            // ignore errors
                            /*if res.is_err() {
                             *                   println!("{:?}", res.err());
                        }*/
                        }
                        res = rx.changed() => {
                            if res.is_err() {
                                break;
                            }
                        }
                    )
                }
            });

            // start reading camera
            let handle = tokio::spawn(async move {
                loop {
                    let client = hyper::Client::new();
                    let res = client.get(stream.1.url.parse().unwrap()).await;
                    if res.is_err() {
                        println!("err: {:?}", res.err().unwrap());
                        break;
                    }
                    let res = res.unwrap();
                    if !res.status().is_success() {
                        eprintln!("HTTP request failed with status {}", res.status());
                        tx.send(default_part.clone()).unwrap();
                        break;
                    }
                    println!("Reading {}", &stream.1.url);
                    let content_type: mime::Mime = res
                    .headers()
                    .get(http::header::CONTENT_TYPE)
                    .unwrap()
                    .to_str()
                    .unwrap()
                    .parse()
                    .unwrap();
                    let stream = res.into_body();
                    let boundary = content_type.get_param(mime::BOUNDARY).unwrap();
                    let mut stream = multipart_stream::parse(stream, boundary.as_str());
                    assert_eq!(content_type.type_(), "multipart");
                    while let Some(p) = stream.next().await {
                        let p = p.unwrap();
                        tx.send(p.clone()).unwrap();
                        //println!("send {:?}", p.headers);
                    }
                }
            });

            handles.push(handle);
        }

        for handle in handles {
            let _ = handle.await;
        }
    }
}
