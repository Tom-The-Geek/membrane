#!/usr/bin/env python3
import os
import subprocess
import sys

key_dir = "keys"

if not os.path.exists(key_dir):
    os.makedirs(key_dir)


def write_openssl_config(server_name: str):
    with open(os.path.join(key_dir, "openssl.cnf"), "w") as f:
        f.write(f"""[ v3_end ]
basicConstraints = critical,CA:false
keyUsage = nonRepudiation, digitalSignature
subjectKeyIdentifier = hash
authorityKeyIdentifier = keyid:always,issuer:always
subjectAltName = @alt_names

[ v3_client ]
basicConstraints = critical,CA:false
keyUsage = nonRepudiation, digitalSignature
extendedKeyUsage = critical, clientAuth
subjectKeyIdentifier = hash
authorityKeyIdentifier = keyid:always,issuer:always

[ v3_inter ]
subjectKeyIdentifier = hash
extendedKeyUsage = critical, serverAuth, clientAuth
basicConstraints = CA:true
keyUsage = cRLSign, keyCertSign, digitalSignature, nonRepudiation, keyEncipherment, dataEncipherment, keyAgreement, keyCertSign, cRLSign

[ alt_names ]
DNS.1 = {server_name}
""")


def generate_ca_key_and_cert(authority_name: str):
    subprocess.call([
        "openssl", "req", "-nodes",
        "-x509",
        "-days", "3650",
        "-newkey", "rsa:4096",
        "-keyout", os.path.join(key_dir, "ca.key"),
        "-out", os.path.join(key_dir, "ca.cert"),
        "-sha256", "-batch",
        "-subj", f"/CN={authority_name} RSA CA",
    ])


def generate_key_and_certificate_request(key_name: str, certificate_name: str):
    subprocess.call([
        "openssl", "req", "-nodes",
        "-newkey", "rsa:4096",
        "-keyout", os.path.join(key_dir, f"{key_name}.key"),
        "-out", os.path.join(key_dir, f"{key_name}.req"),
        "-sha256", "-batch",
        "-subj", f"/CN={certificate_name}"
    ])


def generate_intermediate_certificate(authority_name: str):
    generate_key_and_certificate_request("inter", f"{authority_name} RSA level 2 intermediate")


def generate_end_certificate(server_name: str):
    generate_key_and_certificate_request("end", server_name)


def extract_rsa_key(key_name: str):
    subprocess.call([
        "openssl", "rsa",
        "-in", os.path.join(key_dir, f"{key_name}.key"),
        "-out", os.path.join(key_dir, f"{key_name}.rsa"),
    ])


def sign_certificate(key_name: str, serial: int, cert_type: str, ca_file: str = "inter", days: int = 2000):
    subprocess.call([
        "openssl", "x509", "-req",
        "-in", os.path.join(key_dir, f"{key_name}.req"),
        "-out", os.path.join(key_dir, f"{key_name}.cert"),
        "-CA", os.path.join(key_dir, f"{ca_file}.cert"),
        "-CAkey", os.path.join(key_dir, f"{ca_file}.key"),
        "-sha256",
        "-days", str(days),
        "-set_serial", str(serial),
        "-extensions", f"v3_{cert_type}",
        "-extfile", os.path.join(key_dir, "openssl.cnf"),
    ])


def build_chain(chain_name: str, certificates: list[str]):
    with open(os.path.join(key_dir, chain_name), "w") as f:
        for certificate in certificates:
            with open(os.path.join(key_dir, certificate)) as f2:
                f.write(f2.read())


def generate_ca_and_end(authority_name: str, server_name: str):
    # Setup openssl config
    write_openssl_config(server_name)
    # Generate keys
    generate_ca_key_and_cert(authority_name)
    generate_intermediate_certificate(authority_name)
    generate_end_certificate(server_name)
    # Extract keys
    extract_rsa_key("ca")
    extract_rsa_key("inter")
    extract_rsa_key("end")
    # Sign certificates
    sign_certificate("inter", 123, "inter", ca_file="ca", days=3650)
    sign_certificate("end", 456, "end")
    # Generate chains
    build_chain("end.chain", ["inter.cert", "ca.cert"])
    build_chain("end.fullchain", ["end.cert", "inter.cert", "ca.cert"])


def generate_client(client_name: str):
    # Generate keys
    generate_key_and_certificate_request("client", f"{client_name} client")
    # Extract keys
    extract_rsa_key("client")
    # Sign certificates
    sign_certificate("client", 789, "client")
    # Generate chains
    build_chain("client.chain", ["inter.cert", "ca.cert"])
    build_chain("client.fullchain", ["end.cert", "inter.cert", "ca.cert"])


if len(sys.argv) < 2:
    print(f"Usage: {sys.argv[0]} [ca|client]")
    sys.exit(1)

mode = sys.argv[1]

if mode == "ca":
    generate_ca_and_end(
        input("Enter the authority name (eg. example org): "),
        input("Enter the server name (eg. example.org): "),
    )
elif mode == "client":
    generate_client(
        input("Enter the client name (eg. example): ")
    )
else:
    print(f"Usage: {sys.argv[0]} [ca|client]")
