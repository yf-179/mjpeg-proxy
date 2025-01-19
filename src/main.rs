// Copyright (C) 2021 Scott Lamb <slamb@slamb.org>
// SPDX-License-Identifier: MIT OR Apache-2.0

//! Example `hyper`-based HTTP client which parses a multipart stream.
//! Run with the URL printed by `cargo run --example server` as its one argument.

use std::sync::{Arc, Mutex};

use tokio::{sync::broadcast, task::JoinHandle};

use mjpeg_proxy::config;


#[tokio::main]
async fn main() {

    let servers = config::parse_config("config.yml").expect("Failed to read config");
    let mut handles: Vec<JoinHandle<()>> = Vec::new();

    for (_server_name, mut server) in servers {
        for (_stream_name, stream) in &mut server.streams {
            if !stream.path.starts_with("/") {
                stream.path = format!("/{}", stream.path);
            }
            let (tx, _) = broadcast::channel(1);
            stream.broadcaster = Some(Arc::new(Mutex::new(tx)));
        }

        let handle = tokio::spawn(async move {
            server.run().await
        });
        handles.push(handle);
    }

    for handle in handles {
        let _ = handle.await;
    }

    /*let f = std::fs::File::open("config.yml").unwrap();
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
    }*/

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
