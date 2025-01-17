// Copyright (C) 2021 Scott Lamb <slamb@slamb.org>
// SPDX-License-Identifier: MIT OR Apache-2.0

//! Example `hyper`-based HTTP client which parses a multipart stream.
//! Run with the URL printed by `cargo run --example server` as its one argument.

use std::sync::{Arc, Mutex};

use std::collections::HashMap;

use serde_yaml;

use futures::StreamExt;
use hyper::Body;
use hyper::body::Bytes;
use hyper::header::{CONTENT_LENGTH, CONTENT_TYPE};
use hyper::{self as http, HeaderMap, Request, Response};
use hyper::service::{make_service_fn, service_fn};

use multipart_stream::Part;
use tokio::sync::{broadcast, watch};
use tokio::time::{Duration, interval};

use tokio_stream::wrappers::BroadcastStream;

use lazy_static_include::*;
use lazy_static::lazy_static;

lazy_static_include_bytes! {
    DEFAULT_BODY_CONTENT => "res/no_feed_640x480.jpg",
}

lazy_static! {
    static ref BROADCAST_SENDER: Arc<Mutex<broadcast::Sender<Part>>> = {let (tx, _) = broadcast::channel(1); Arc::new(Mutex::new(tx))};
}

//const DEFAULT_BODY_CONTENT_LENGTH = 8857;

const BOUNDARY_STRING :&str = "-----mjpeg-proxy-boundary-----";

#[derive(Clone)]
struct StreamConfig {
    path: String,
    url: String,
    fps: u64,
    broadcaster: Arc<Mutex<broadcast::Sender<Part>>>,
}

struct Server {
    config: HashMap<String, StreamConfig>,
    port: u64,
    address: String,
}


impl Server {
    pub async fn run(&self) {
        let addr = format!("{}:{}", &self.address, self.port).parse().unwrap();
        let mut handles = Vec::new();
        let mut stream_config = Arc::new(HashMap::new());
        let mut config = HashMap::new();
        for v in self.config.iter() {
            Arc::get_mut(&mut stream_config).unwrap().insert(v.1.path.clone(), v.1.broadcaster.clone());
            config.insert(v.1.path.clone(), v.1.clone());
        }

        let make_svc = make_service_fn({
            move |_conn| {
                futures::future::ok::<_, std::convert::Infallible>(
                    service_fn({
                        let v1 = Arc::clone(&stream_config);
                        move |req| {
                            let v2 = Arc::clone(&v1);
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
                let mut interval = interval(Duration::from_micros(1000000 / stream.1.fps));
                loop {
                    tokio::select!(
                        _ = interval.tick() => {
                            let latest = rx.borrow().clone();
                            //println!("recv: {:?}", latest.headers);
                            let btx = stream.1.broadcaster.lock().unwrap();
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

async fn serve(req: Request<Body>, config: Arc<HashMap<String, Arc<Mutex<broadcast::Sender<Part>>>>>) -> Result<Response<Body>, Box<dyn std::error::Error + Send + Sync>> {
    let path = req.uri().to_string();
    println!("Accessed: {}", path);
    let res = config.get(&path);
    if res.is_some() {
        let recv = res.unwrap().lock().unwrap().subscribe();
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


#[tokio::main]
async fn main() {


    let f = std::fs::File::open("config.yml").unwrap();
    let data: serde_yaml::Value = serde_yaml::from_reader(f).unwrap();

    let servers = data.as_mapping().unwrap();
    let mut port = 8080;
    let mut address = "127.0.0.1".to_string();
    let mut handles = Vec::new();
    for server in servers {
        let info = server.1;
        let mut config = HashMap::new();
        for sinfo in info.as_mapping().unwrap() {
            let k = sinfo.0.as_str().unwrap();
            if k == "address" {
                address = info.get("address").unwrap().as_str().unwrap().to_string();
            }
            else if k == "port" {
                port = info.get("port").unwrap().as_u64().unwrap();
            } else {
                let mut path = k.to_string();
                let stream = sinfo.1.as_mapping().unwrap();
                let fps = stream.get("fps").unwrap().as_u64().unwrap();
                let url = stream.get("url").unwrap().as_str().unwrap().to_string();
                let res = stream.get("path");
                if res.is_some() {
                    path = res.unwrap().as_str().unwrap().to_string();
                }
                if !path.starts_with("/") {
                    path = format!("/{}", path);
                }
                let (tx, _) = broadcast::channel(1);
                let broadcaster = Arc::new(Mutex::new(tx));
                //println!("{} {} {}", path, url, fps);
                config.insert(path.clone(), StreamConfig{path, url, fps, broadcaster});
            }
            //println!("{:?}", sinfo);
        }


        for stream in config.iter() {
            println!("{} {} {}", stream.1.path, stream.1.url, stream.1.fps)
        }
        let address = address.clone();

        let handle = tokio::spawn(async move {
            let server = Server{config, port, address};
            server.run().await
        });

        handles.push(handle);

    }

    for handle in handles {
        let _ = handle.await;
    }

    return;
/*

    let url = match std::env::args().nth(1) {
        Some(u) => http::Uri::try_from(u).unwrap(),
        None => {
            eprintln!("Usage: mjpeg-proxy URL [fps (default 15)]");
            std::process::exit(1);
        }
    };
    let fps = match std::env::args().nth(2) {
        Some(u) => u.parse().unwrap(),
        None => {
            15
        }
    };


    let client = hyper::Client::new();
    let res = client.get(url).await.unwrap();
    if !res.status().is_success() {
        eprintln!("HTTP request failed with status {}", res.status());
        std::process::exit(1);
    }
    let content_type: mime::Mime = res
    .headers()
    .get(http::header::CONTENT_TYPE)
    .unwrap()
    .to_str()
    .unwrap()
    .parse()
    .unwrap();
    assert_eq!(content_type.type_(), "multipart");
    let default_body = Bytes::from(&DEFAULT_BODY_CONTENT[..]);
    let mut headers = HeaderMap::new();
    headers.insert(CONTENT_TYPE, "image/jpeg".parse().unwrap());
    headers.insert(CONTENT_LENGTH, "8857".parse().unwrap());
    let default_part = Part{headers, body: default_body};
    let boundary = content_type.get_param(mime::BOUNDARY).unwrap();
    let stream = res.into_body();
    let mut stream = multipart_stream::parse(stream, boundary.as_str());
    let (tx, mut rx) = watch::channel(default_part.clone());
    let mut interval = interval(Duration::from_micros(1000000 / fps));
    */
    /*let (server_tx, mut server_rx) = tokio::sync::mpsc::channel(1);

    tokio::spawn(async move {
        let addr = ([0, 0, 0, 0], 8080).into();
        let (btx, _) = broadcast::channel(1);
        let btx = Arc::new(Mutex::new(btx));
        let make_svc = make_service_fn(move |_conn| {
            let mut brx = btx.lock().unwrap();
            let brx = brx.subscribe();
            futures::future::ok::<_, std::convert::Infallible>(service_fn(move |req| serve(req, brx)))
        });
        let server = hyper::Server::bind(&addr).serve(make_svc);
        println!("Serving on http://{}", server.local_addr());
        server.await.unwrap();
    });
 */


}
