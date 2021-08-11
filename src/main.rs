// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at http://mozilla.org/MPL/2.0/.

#[macro_use]
extern crate log;

use std::sync::Arc;

use rustls::{AllowAnyAuthenticatedClient, RootCertStore};
use tokio::io;
use tokio::net::{TcpListener, TcpStream};
use tokio::sync::mpsc;
use tokio_rustls::{rustls, TlsAcceptor, TlsConnector};
use tokio_rustls::rustls::ClientConfig;
use tokio_rustls::webpki::DNSNameRef;

use crate::config::{CommonConfig, Config, load_certs, load_config, load_private_key};

mod config;

fn make_tunnel_end_config(config: &CommonConfig) -> Arc<rustls::ServerConfig> {
    let client_auth = {
        let roots = load_certs(&config.client_certificates_file);
        let mut client_auth_roots = RootCertStore::empty();
        for root in roots {
            client_auth_roots.add(&root).unwrap();
        }
        AllowAnyAuthenticatedClient::new(client_auth_roots)
    };

    let certs = load_certs(&config.server_certificates_file);
    let private_key = load_private_key(&config.key_file);

    let mut config = rustls::ServerConfig::new(client_auth);
    config.set_single_cert(certs, private_key)
        .expect("invalid certificates/private key");

    config.key_log = Arc::new(rustls::KeyLogFile::new());

    Arc::new(config)
}

fn make_tunnel_start_config(config: &CommonConfig) -> Arc<rustls::ClientConfig> {
    let client_certs = load_certs(&config.client_certificates_file);
    let client_key = load_private_key(&config.key_file);

    let mut client_config = ClientConfig::new();
    client_config.set_single_client_cert(client_certs, client_key)
        .expect("invalid certificates/private key");

    let certs = load_certs(&config.server_certificates_file);
    for cert in certs {
        client_config.root_store.add(&cert)
            .expect("invalid certificate");
    }

    client_config.key_log = Arc::new(rustls::KeyLogFile::new());

    Arc::new(client_config)
}

async fn tunnel_end_main(config: &CommonConfig) -> io::Result<()> {
    let server_config = make_tunnel_end_config(&config);

    let acceptor = TlsAcceptor::from(server_config);
    let listener = TcpListener::bind(format!("0.0.0.0:{}", config.listen_port)).await?;

    loop {
        let (stream, peer_addr) = listener.accept().await?;
        info!("new connection from {}", peer_addr);

        let acceptor = acceptor.clone();
        let config = config.clone();

        let fut = async move {
            let stream = acceptor.accept(stream).await?;
            let backend_stream = TcpStream::connect(format!("{}:{}", config.target_host, config.target_port)).await?;
            let (mut rd, mut wr) = io::split(stream);
            let (mut back_rd, mut back_wr) = io::split(backend_stream);

            let (tx, mut rx) = mpsc::channel(1);
            let tx2 = tx.clone();

            let mut handles = Vec::with_capacity(2);

            handles.push(tokio::spawn(async move {
                if let Err(e) = io::copy(&mut rd, &mut back_wr).await {
                    error!("c->b error: {:?}", e);
                }
                tx.send(true).await.unwrap();
            }));

            handles.push(tokio::spawn(async move {
                if let Err(e) = io::copy(&mut back_rd, &mut wr).await {
                    error!("b->c error: {:?}", e);
                }
                tx2.send(true).await.unwrap();
            }));

            if rx.recv().await.is_some() {
                for handle in handles {
                    handle.abort();
                }
            }

            Ok(()) as io::Result<()>
        };

        tokio::spawn(async move {
            if let Err(e) = fut.await {
                error!("connection err {:?}", e);
            }
        });
    }
}

async fn tunnel_start_main(config: &CommonConfig) -> io::Result<()> {
    let client_config = make_tunnel_start_config(&config);

    let listener = TcpListener::bind(format!("127.0.0.1:{}", config.listen_port)).await?;

    loop {
        let (stream, peer_addr) = listener.accept().await?;
        info!("new connection from {}", peer_addr);

        let client_config = client_config.clone();
        let config = config.clone();

        let fut = async move {
            info!("connecting to backend at {}:{}...", config.target_host, config.target_port);
            let backend_stream = TcpStream::connect(
                format!("{}:{}", config.target_host, config.target_port)).await?;

            info!("setting up tls connector...");
            let tls_connector = TlsConnector::from(client_config.clone());

            let domain = DNSNameRef::try_from_ascii_str(&config.target_host)
                .map_err(|_| io::Error::new(io::ErrorKind::InvalidInput, "invalid dnsname"))?;

            info!("initiating tls connection...");
            let backend_stream = tls_connector.connect(domain, backend_stream).await?;

            info!("running copy loop");
            let (mut rd, mut wr) = io::split(stream);
            let (mut backend_rd, mut backend_wr) = io::split(backend_stream);

            let (tx, mut rx) = mpsc::channel(1);
            let tx2 = tx.clone();

            let mut handles = Vec::with_capacity(2);

            handles.push(tokio::spawn(async move {
                if let Err(e) = io::copy(&mut rd, &mut backend_wr).await {
                    error!("c->b error: {:?}", e);
                }
                tx.send(true).await.unwrap();
            }));

            handles.push(tokio::spawn(async move {
                if let Err(e) = io::copy(&mut backend_rd, &mut wr).await {
                    error!("b->c error: {:?}", e);
                }
                tx2.send(true).await.unwrap();
            }));

            if rx.recv().await.is_some() {
                for handle in handles {
                    handle.abort();
                }
            }

            Ok(()) as io::Result<()>
        };

        tokio::spawn(async move {
            if let Err(e) = fut.await {
                error!("connection err {:?}", e);
            }
        });
    }
}

#[tokio::main]
async fn main() -> io::Result<()> {
    env_logger::init();
    info!("Hello, world!");

    let membrane_config = load_config();
    match membrane_config {
        Config::Gateway(config) => tunnel_end_main(&config).await,
        Config::Tunneler(config) => tunnel_start_main(&config).await,
    }
}
