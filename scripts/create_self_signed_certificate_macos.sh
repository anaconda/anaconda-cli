#!/bin/bash

set -eu

mkdir -p "${ROOT_DIR}"

# Array assignment may leave the first element empty, so run cut twice
openssl_lib=$(openssl version | cut -d' ' -f1)
openssl_version=$(openssl version | cut -d' ' -f2)
if [[ "${openssl_lib}" == "OpenSSL" ]] && [[ "${openssl_version}" == 3.* ]]; then
    legacy='-legacy'
fi

keyusage="codeSigning"
certtype="1.2.840.113635.100.6.1.13"
commonname="${APPLICATION_SIGNING_ID}"
password="${APPLICATION_SIGNING_PASSWORD}"
keyfile="${ROOT_DIR}/application.key"
p12file="${ROOT_DIR}/application.p12"
crtfile="${ROOT_DIR}/application.crt"

openssl genrsa -out "${keyfile}" 2048
openssl req -x509 -new -key "${keyfile}"\
    -out "${crtfile}"\
    -sha256\
    -days 1\
    -subj "/C=XX/ST=State/L=City/O=Company/OU=Org/CN=${commonname}/emailAddress=somebody@somewhere.com"\
    -addext "basicConstraints=critical,CA:FALSE"\
    -addext "extendedKeyUsage=critical,${keyusage}"\
    -addext "keyUsage=critical,digitalSignature"\
    -addext "${certtype}=critical,DER:0500"

# shellcheck disable=SC2086
openssl pkcs12 -export\
    -out "${p12file}"\
    -inkey "${keyfile}"\
    -in "${crtfile}"\
    -passout pass:"${password}"\
    ${legacy}
