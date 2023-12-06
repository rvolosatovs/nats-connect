# Descripton

`nats-connect` serves one and only simple purpose - provide a bidirectional communication stream between two [NATS.io](https://nats.io/) nodes akin to a TCP stream.

`nats-connect` connections are initialized by the client - the server "accepts" connections on a particular topic and performs the "handshake" procedure with a client on request.
