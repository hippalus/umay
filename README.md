# Umay: Rust-based TLS Proxy and Load Balancer

Umay is an experimental, high-performance TLS proxy and load balancer written in Rust. It's designed to provide secure
and efficient traffic management for microservices architectures.

For those interested in the origin of the name "Umay," you can learn more about
it [here](https://en.wikipedia.org/wiki/Umay).

## Features

- TLS termination and proxying
- Multiple load balancing algorithms (Round Robin, Random, Least Connection, Consistent Hashing)
- Dynamic backend discovery (DNS-based and local configuration)
- Configurable via TOML files and environment variables
- Metrics collection
- Graceful shutdown

## Configuration

Umay can be configured using TOML files and environment variables. See `config.rs` for available options.

## Quick Start
To quickly run and test Umay using Docker Compose:

- Ensure you have Docker and Docker Compose installed.
- Clone this repository and navigate to the project directory.
- Start the services:
```bash
docker-compose up --build
```

Test the proxy:

```bash
"Hello, Umay!" | openssl s_client -connect localhost:9994 -ign_eof
```

If working correctly, you should see a TLS handshake followed by an echo response from one of the backend servers.

To stop the services:
```bash
docker-compose down
```
## Development Status

This project is under active development and is considered experimental. Use in production environments is not
recommended at this time.

## Contributing

Contributions are welcome! Please review our [Project Plan](docs/PROJECT_PLAN) before submitting a Pull Request. We
appreciate any feedback, bug reports, or feature requests.

## License

This project is licensed under the [MIT License](LICENSE).

## Acknowledgements

We would like to acknowledge the Rust community for their excellent libraries and resources that have greatly supported
this project. Additionally, inspiration was drawn from projects
like [linkerd2-proxy](https://github.com/linkerd/linkerd2-proxy), [Pingora by Cloudflare](https://github.com/cloudflare/pingora),
and [Istio ztunnel](https://github.com/istio/ztunnel).

