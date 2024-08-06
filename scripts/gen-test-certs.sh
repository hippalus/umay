#!/bin/bash

# Define default values for certificate details
CERT_DIR="./test-certs"
DEFAULT_ORG="Umay Inc."
DEFAULT_LOCALITY="Amsterdam"
DEFAULT_STATE="Noord-Holland"
DEFAULT_COUNTRY="NL"
DEFAULT_EE_NAME="umay"
DEFAULT_EE_NS="default-ns"
DEFAULT_CP_NS="default-cp"

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

# Create a list of DNS names from 0 to 9
if [ -z "$9" ]; then
  DNS_NAMES=""
  for i in {0..9}; do
    DNS_NAMES="${DNS_NAMES}${EE_NAME}-${i}.${EE_NS}.serviceaccount.identity.${CP_NS}.cluster.local,"
  done
  DNS_NAMES="${DNS_NAMES%,}"
else
  DNS_NAMES="$9"
fi

echo "Using the following settings:"
echo "Organization: $ORG"
echo "Locality: $LOCALITY"
echo "State: $STATE"
echo "Country: $COUNTRY"
echo "Endpoint Name: $EE_NAME"
echo "Endpoint Namespace: $EE_NS"
echo "Control Plane Namespace: $CP_NS"
echo "Common Name (CN): $CN"
echo "DNS Names: $DNS_NAMES"

# Create the certificate directory if it doesn't exist
mkdir -p $CERT_DIR

# Paths for certificate and key files
CA_KEY="${CERT_DIR}/ca.key"
CA_CERT="${CERT_DIR}/ca.crt"
SERVER_KEY="${CERT_DIR}/${EE_NAME}.key"
SERVER_CSR="${CERT_DIR}/${EE_NAME}.csr"
SERVER_CERT="${CERT_DIR}/${EE_NAME}.crt"

# Clean up old files
rm -f $CA_KEY $CA_CERT $SERVER_KEY $SERVER_CSR $SERVER_CERT

# 1. Create a CA private key
openssl genpkey -algorithm RSA -out $CA_KEY -pkeyopt rsa_keygen_bits:2048

# 2. Create a CA certificate
openssl req -x509 -new -nodes -key $CA_KEY -sha256 -days 3650 -out $CA_CERT \
  -subj "/C=${COUNTRY}/ST=${STATE}/L=${LOCALITY}/O=${ORG}/CN=CA"

# 3. Create a server private key
openssl genpkey -algorithm RSA -out $SERVER_KEY -pkeyopt rsa_keygen_bits:2048

# 4. Create a server certificate signing request (CSR)
openssl req -new -key $SERVER_KEY -out $SERVER_CSR \
  -subj "/C=${COUNTRY}/ST=${STATE}/L=${LOCALITY}/O=${ORG}/CN=${CN}"

# 5. Create a server certificate with Subject Alternative Names (SAN)
SAN_CONFIG="${CERT_DIR}/san_config.cnf"
echo "subjectAltName = DNS:${DNS_NAMES//,/\,DNS:}" > $SAN_CONFIG

openssl x509 -req -in $SERVER_CSR -CA $CA_CERT -CAkey $CA_KEY -CAcreateserial -out $SERVER_CERT -days 3650 -sha256 -extfile $SAN_CONFIG

# Cleanup
rm -f $SERVER_CSR $SAN_CONFIG

echo "Certificates generated:"
echo "CA Certificate: $CA_CERT"
echo "Server Certificate: $SERVER_CERT"
echo "Server Private Key: $SERVER_KEY"
