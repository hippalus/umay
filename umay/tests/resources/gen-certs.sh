#!/bin/sh
#
# Requires:
# go get -u github.com/cloudflare/cfssl/cmd/cfssl
# go get -u github.com/cloudflare/cfssl/cmd/cfssljson
#
set -euox pipefail

ca() {
  name=$1
  filename=$2

  echo "{\"names\":[{\"CN\": \"${name}\",\"OU\":\"None\"}], \"ca\": {\"expiry\": \"87600h\"}}" \
    | cfssl genkey -initca - \
    | cfssljson -bare "${filename}"

  rm "${filename}.csr"
}

ee() {
  ca_name=$1
  ee_name=$2
  ee_ns=$3
  cp_ns=$4

  hostname="${ee_name}.${ee_ns}.serviceaccount.identity.${cp_ns}.cluster.local"

  ee="${ee_name}-${ee_ns}-${ca_name}"
  echo '{}' \
    | cfssl gencert -ca "${ca_name}.pem" -ca-key "${ca_name}-key.pem" -hostname "${hostname}" -config=ca-config.json - \
    | cfssljson -bare "${ee}"
  mkdir -p "${ee}"

  # No need to convert to PKCS#8, keep the key in PEM format
  mv "${ee}-key.pem" "${ee}/key.pem"

  openssl x509 -inform pem -outform der \
    -in "${ee}.pem" \
    -out "${ee}/crt.der"
  rm "${ee}.pem"

  ## TODO DER-encode?
  #openssl x509 -inform pem -outform der \
  #  -in "${ee}.csr" \
  #  -out "${ee}/csr.der"
  mv "${ee}.csr" "${ee}/csr.pem"
}

ca 'Cluster-local CA 1' ca

ee ca default default umay