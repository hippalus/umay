# Umay

## 1. Project Overview

The Umay SSL Proxy aims to develop a high-performance, secure proxy server engineered in Rust, supporting multiple
protocols and providing advanced features such as load balancing, rate limiting, and comprehensive metrics/tracing
collection. This experimental project is designed to investigate cutting-edge network protocols, proxy load balancing
algorithms, and related technologies to push the boundaries of modern proxy servers.

## 2. Requirements

### 2.1 Functional Requirements

**Protocol Support:**

- HTTPS
- WSS (WebSocket over TLS)
- TCP (MQTT over TLS, etc.)
- HTTP (with optional upgrade to HTTPS)

**SSL/TLS Termination and Initiation:**

- Terminate incoming SSL/TLS connections
- Initiate new SSL/TLS connections to backend servers (default PLAINTEXT)

**Server Name Indication (SNI):**

- Support for SNI to handle multiple SSL certificates for different domains on the same IP address

**Load Balancing:**

- Implement smart backend selection algorithms
- Support multiple load balancing strategies (e.g., round-robin, least connections)
- Automatic, latency-aware, layer-7 load balancing
- Automatic layer-4 load balancing for non-HTTP traffic

**Protocol Detection:**

- Automatically detect and route incoming connections based on protocol

**Service Discovery:**

- DNS Discovery
- Resolve backend server hostnames
- Support periodic DNS resolution updates
- Support automatic backend updates based on service registry changes

**Rate Limiting:**

- Implement IP-based rate limiting
- Support configurable rate limit rules

**Metrics and Monitoring:**

- Collect and export advanced metrics (e.g., connection count, data transfer, latency)
- Integrate with Prometheus for metrics exposure
- Automatic Prometheus metrics export for HTTP and TCP traffic

**Configuration Management:**

- Support configuration via file and environment variables
- Allow dynamic configuration updates without restart

**Logging and Error Handling:**

- Implement comprehensive logging
- Provide detailed error reporting and handling

**Kubernetes Support:**

- Provide Kubernetes deployment manifests
- Support for Kubernetes Service and Ingress resources
- Automatic scaling based on resource usage

**Additional Features (Main Expected Functionality):**

- Transparent, zero-config proxying for HTTP, HTTP/2, and arbitrary TCP protocols
- Transparent, zero-config WebSocket proxying

### 2.2 Non-Functional Requirements

**Performance:**

- Handle at least 10,000 concurrent connections
- Maintain low latency (< 50ms added by proxy)

**Security:**

- Support latest TLS versions (1.2 and 1.3)
- Implement secure defaults for SSL/TLS configuration
- Regular security audits and updates

**Scalability:**

- Horizontal scalability to multiple instances
- Efficient resource utilization

**Reliability:**

- 99.99% uptime
- Graceful handling of backend failures

**Maintainability:**

- Well-documented code
- Modular architecture for easy updates and extensions

**Compliance:**

- Adhere to relevant data protection regulations (e.g., GDPR, CCPA)

## 3. Test Cases

**TLS Handshake:**

- Verify successful TLS handshake with clients and backends
- Test with different TLS versions and cipher suites

**Protocol Routing:**

- Ensure correct routing for HTTPS, WSS, MQTTS, and HTTP
- Test protocol upgrade scenarios (HTTP to HTTPS)

**Load Balancing:**

- Verify even distribution of connections across backends
- Test failover scenarios when a backend is unreachable

**Rate Limiting:**

- Confirm rate limiting is applied correctly
- Test rate limit behavior under high load

**Metrics Collection:**

- Validate accuracy of collected metrics
- Ensure Prometheus endpoint exposes all required metrics

**Configuration:**

- Test loading of configuration from file and environment variables
- Verify dynamic configuration updates

**Performance:**

- Load tests to verify concurrent connection handling
- Measure and validate latency under various load conditions

**Security:**

- Verify proper handling of invalid SSL certificates

**Error Handling:**

- Test proxy behavior under various error conditions (e.g., backend unavailable, malformed requests)

**Service Discovery:**

- Verify integration with service discovery mechanisms
- Test automatic updates of backend servers based on service registry changes

**Kubernetes Support:**

- Test integration with Kubernetes Service and Ingress resources

## 4. Development Roadmap

**Basic TCP Proxy**

- Implement basic TCP forwarding
- Set up project structure and testing framework

**TLS Support**

- Add TLS termination and initiation
- Implement secure TLS configuration

**Protocol Support**

- Develop protocol detection mechanism
- Implement handlers for HTTPS, WSS, MQTTS, and HTTP

**Load Balancing**

- Create load balancing module
- Implement multiple load balancing strategies

**DNS Resolution**

- Add DNS resolution for backend servers
- Implement periodic DNS updates

**Service Discovery**

- Integrate with service discovery mechanisms
- Implement automatic backend updates based on service registry changes

**Rate Limiting**

- Develop rate limiting module
- Implement configurable rate limiting rules

**Metrics and Monitoring**

- Set up metrics collection
- Integrate with Prometheus

**Configuration Management**

- Implement configuration loading from file and environment
- Develop dynamic configuration update mechanism

---
