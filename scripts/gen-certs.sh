#!/bin/bash

# Define default values for certificate details
CERT_DIR="./certs"
DEFAULT_ORG="Umay Inc."
DEFAULT_LOCALITY="Amsterdam"
DEFAULT_STATE="Noord-Holland"
DEFAULT_COUNTRY="NL"
DEFAULT_EE_NAME="umay"
DEFAULT_EE_NS="default"
DEFAULT_CP_NS="prod"
DEFAULT_CN="${DEFAULT_EE_NAME}.${DEFAULT_EE_NS}.serviceaccount.identity.${DEFAULT_CP_NS}.cluster.local"
DEFAULT_DNS_NAMES="${DEFAULT_EE_NAME}-0.${DEFAULT_EE_NS}.serviceaccount.identity.${DEFAULT_CP_NS}.cluster.local,${DEFAULT_EE_NAME}-1.${DEFAULT_EE_NS}.serviceaccount.identity.${DEFAULT_CP_NS}.cluster.local,${DEFAULT_EE_NAME}-2.${DEFAULT_EE_NS}.serviceaccount.identity.${DEFAULT_CP_NS}.cluster.local,${DEFAULT_EE_NAME}-3.${DEFAULT_EE_NS}.serviceaccount.identity.${DEFAULT_CP_NS}.cluster.local,${DEFAULT_EE_NAME}-4.${DEFAULT_EE_NS}.serviceaccount.identity.${DEFAULT_CP_NS}.cluster.local,${DEFAULT_EE_NAME}-5.${DEFAULT_EE_NS}.serviceaccount.identity.${DEFAULT_CP_NS}.cluster.local,${DEFAULT_EE_NAME}-6.${DEFAULT_EE_NS}.serviceaccount.identity.${DEFAULT_CP_NS}.cluster.local,${DEFAULT_EE_NAME}-7.${DEFAULT_EE_NS}.serviceaccount.identity.${DEFAULT_CP_NS}.cluster.local,${DEFAULT_EE_NAME}-8.${DEFAULT_EE_NS}.serviceaccount.identity.${DEFAULT_CP_NS}.cluster.local,${DEFAULT_EE_NAME}-9.${DEFAULT_EE_NS}.serviceaccount.identity.${DEFAULT_CP_NS}.cluster.local"

# Allow overriding defaults with command-line arguments
ORG="${1:-$DEFAULT_ORG}"
LOCALITY="${2:-$DEFAULT_LOCALITY}"
STATE="${3:-$DEFAULT_STATE}"
COUNTRY="${4:-$DEFAULT_COUNTRY}"
EE_NAME="${5:-$DEFAULT_EE_NAME}"
EE_NS="${6:-$DEFAULT_EE_NS}"
CP_NS="${7:-$DEFAULT_CP_NS}"

# Generate CN and DNS names based on the provided parameters
CN="${8:-${EE_NAME}.${EE_NS}.serviceaccount.identity.${CP_NS}.cluster.local}"
DNS_NAMES="${9:-${EE_NAME}-0.${EE_NS}.serviceaccount.identity.${CP_NS}.cluster.local,${EE_NAME}-1.${EE_NS}.serviceaccount.identity.${CP_NS}.cluster.local,${EE_NAME}-2.${EE_NS}.serviceaccount.identity.${CP_NS}.cluster.local,${EE_NAME}-3.${EE_NS}.serviceaccount.identity.${CP_NS}.cluster.local,${EE_NAME}-4.${EE_NS}.serviceaccount.identity.${CP_NS}.cluster.local,${EE_NAME}-5.${EE_NS}.serviceaccount.identity.${CP_NS}.cluster.local,${EE_NAME}-6.${EE_NS}.serviceaccount.identity.${CP_NS}.cluster.local,${EE_NAME}-7.${EE_NS}.serviceaccount.identity.${CP_NS}.cluster.local,${EE_NAME}-8.${EE_NS}.serviceaccount.identity.${CP_NS}.cluster.local,${EE_NAME}-9.${EE_NS}.serviceaccount.identity.${CP_NS}.cluster.local}"

# Create the directory to store certificates and keys
mkdir -p "${CERT_DIR}"

# Define file names for keys and certificates
ROOT_KEY="${CERT_DIR}/root-ca.key"
ROOT_CERT="${CERT_DIR}/root-ca.crt"
INTERMEDIATE_KEY="${CERT_DIR}/intermediate-ca.key"
INTERMEDIATE_CERT="${CERT_DIR}/intermediate-ca.crt"
SERVER_KEY="${CERT_DIR}/server.key"
SERVER_CERT="${CERT_DIR}/server.crt"
SERVER_CSR="${CERT_DIR}/server.csr"
CLIENT_KEY="${CERT_DIR}/client.key"
CLIENT_CERT="${CERT_DIR}/client.crt"
CLIENT_CSR="${CERT_DIR}/client.csr"

# Generate Root CA key and certificate
openssl genrsa -out "${ROOT_KEY}" 4096
openssl req -x509 -new -nodes -key "${ROOT_KEY}" -sha256 -days 3650 -out "${ROOT_CERT}" -subj "/CN=root-ca/O=${ORG}/L=${LOCALITY}/ST=${STATE}/C=${COUNTRY}"

# Generate Intermediate CA key and certificate
openssl genrsa -out "${INTERMEDIATE_KEY}" 4096
openssl req -new -key "${INTERMEDIATE_KEY}" -out "${CERT_DIR}/intermediate-ca.csr" -subj "/CN=intermediate-ca/O=${ORG}/L=${LOCALITY}/ST=${STATE}/C=${COUNTRY}"
openssl x509 -req -in "${CERT_DIR}/intermediate-ca.csr" -CA "${ROOT_CERT}" -CAkey "${ROOT_KEY}" -CAcreateserial -out "${INTERMEDIATE_CERT}" -days 1825 -sha256

# Generate Server key and CSR (Certificate Signing Request)
openssl genrsa -out "${SERVER_KEY}" 2048
openssl req -new -key "${SERVER_KEY}" -out "${SERVER_CSR}" -subj "/CN=${CN}/O=${ORG}/L=${LOCALITY}/ST=${STATE}/C=${COUNTRY}"

# Generate Server certificate signed by Intermediate CA
openssl x509 -req -in "${SERVER_CSR}" -CA "${INTERMEDIATE_CERT}" -CAkey "${INTERMEDIATE_KEY}" -CAcreateserial -out "${SERVER_CERT}" -days 365 -sha256 \
-extfile <(printf "subjectAltName=DNS:${DNS_NAMES//,/DNS:}")

# Generate Client key and CSR (for mTLS)
openssl genrsa -out "${CLIENT_KEY}" 2048
openssl req -new -key "${CLIENT_KEY}" -out "${CLIENT_CSR}" -subj "/CN=client/O=${ORG}/L=${LOCALITY}/ST=${STATE}/C=${COUNTRY}"

# Generate Client certificate signed by Intermediate CA
openssl x509 -req -in "${CLIENT_CSR}" -CA "${INTERMEDIATE_CERT}" -CAkey "${INTERMEDIATE_KEY}" -CAcreateserial -out "${CLIENT_CERT}" -days 365 -sha256

# Output
echo "Certificates and keys have been generated in the ${CERT_DIR} directory."
echo "Root CA: ${ROOT_CERT}"
echo "Intermediate CA: ${INTERMEDIATE_CERT}"
echo "Server Cert: ${SERVER_CERT}"
echo "Client Cert: ${CLIENT_CERT}"
