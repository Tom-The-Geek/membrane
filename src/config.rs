use std::fs;
use std::fs::File;
use std::io::{BufReader, Read};
use std::path::Path;

use serde::{Deserialize, Serialize};
use tokio_rustls::rustls;
use tokio_rustls::rustls::internal::pemfile::{certs, rsa_private_keys};

#[derive(Deserialize, Serialize, Clone, Debug)]
#[serde(rename_all = "snake_case", tag = "mode", content = "config")]
pub enum Config {
    Tunneler(CommonConfig),
    Gateway(CommonConfig),
}

#[derive(Deserialize, Serialize, Clone, Debug)]
pub struct CommonConfig {
    pub server_certificates_file: String,
    pub key_file: String,
    pub client_certificates_file: String,

    pub listen_port: u16,

    pub target_host: String,
    pub target_port: u16,
}

impl Default for Config {
    fn default() -> Self {
        Self::Gateway(CommonConfig {
            server_certificates_file: "keys/end.fullchain".to_string(),
            key_file: "keys/end.rsa".to_string(),
            client_certificates_file: "keys/client.chain".to_string(),

            listen_port: 20443,

            target_host: "localhost".to_string(),
            target_port: 20000,
        })
    }
}

pub fn load_config() -> Config {
    let path = Path::new("config.toml");
    if path.exists() {
        let mut buf = String::new();
        File::open(path).expect("failed to open config")
            .read_to_string(&mut buf).expect("failed to read config");
        toml::from_str(&buf).expect("failed to parse config")
    } else {
        let config = Config::default();

        let content = toml::to_string(&config).expect("failed to write default config");
        fs::write(path, content).expect("failed to write default config");

        config
    }
}

pub fn load_certs(filename: &str) -> Vec<rustls::Certificate> {
    let certfile = fs::File::open(filename).expect("cannot open certificate file");
    let mut reader = BufReader::new(certfile);
    certs(&mut reader)
        .expect("failed to read certificates file")
}

pub fn load_private_key(filename: &str) -> rustls::PrivateKey {
    let keyfile = fs::File::open(filename).expect("cannot open private key file");
    let mut reader = BufReader::new(keyfile);
    rsa_private_keys(&mut reader).expect("failed to read private key file")
        .first().expect("no private keys in file").clone()
}
