set -e

openssl req -nodes -new -x509 -keyout ca-key.pem -out ca-cert.pem -days 65536 -config config/ca.cnf

openssl req -new -nodes -newkey ec -pkeyopt ec_paramgen_curve:secp384r1 -keyout server-key.pem -out server.csr -config config/server.cnf
openssl x509 -days 65536 -req -in server.csr -CA ca-cert.pem -CAkey ca-key.pem -CAcreateserial -out server-cert.pem -extensions req_ext -extfile config/server.cnf

openssl req -new -nodes -newkey ec -pkeyopt ec_paramgen_curve:secp384r1 -keyout client-key.pem -out client.csr -config config/client.cnf
openssl x509 -days 65536 -req -in client.csr -CA ca-cert.pem -CAkey ca-key.pem -CAcreateserial -out client-cert.pem -extensions req_ext -extfile config/client.cnf
