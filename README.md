# Wakezilla 🦖
<img width="2698" height="2012" alt="image" src="https://github.com/user-attachments/assets/667eedeb-431c-4aa2-bf7a-3eadd4221452" />

A simple Wake-on-LAN solution with HTTP proxy capabilities, remote machine management, and automatic shutdown features.

## Features

- **Wake-on-LAN**: Send magic packets to wake sleeping machines
- **TCP Proxy**: Forward ports to remote machines with automatic WOL
- **Web Interface**: Manage machines, ports, and monitor activity through a web dashboard
- **Automatic Shutdown**: Automatically turn off machines after inactivity periods
- **Network Scanner**: Discover machines on your local network

## Installation

### Server Installation

1. **Install Rust**:
   ```bash
   curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh
   source $HOME/.cargo/env
   ```

2. **Clone and Build**:
   ```bash
   git clone <repository-url>
   cd wakezilla
   cargo build --release
   ```

3. **Configure the Server**:
   Create a `machines.json` file (optional, will be created automatically):
   ```json
   []
   ```

4. **Run the Server**:
   ```bash
   ./target/release/wakezilla --server
   ```
   
   By default, the web interface runs on port 3000.

### Client Installation
   make sure the machine was configured with wake on lan.
1. **Install Rust** (if not already installed):
   ```bash
   curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh
   source $HOME/.cargo/env
   ```

2. **Clone and Build**:
   ```bash
   git clone <repository-url>
   cd wakezilla
   cargo build --release
   ```
## Usage

### Starting the Proxy Server
```bash
# Basic server start
./target/release/wakezilla proxy-server

# With custom port
./target/release/wakezilla proxy-server --port 8080

```

### Starting the Client
```bash
# Connect to server
./target/release/wakezilla client-server 

# With custom port
./target/release/wakezilla client-server --port 8080

```

### Web Interface
Access the web interface at `http://<server-ip>:3000` to:
- Add and manage machines
- Configure port forwards
- View network scan results
- Send WOL packets manually
- Configure automatic shutdown settings

### Adding Machines
1. Navigate to the web interface
2. Click "Add Machine" or use the network scanner
3. Fill in MAC address, IP, and name
4. Configure:
   - Turn-off port (if remote shutdown is needed)
   - Request rate limiting (requests per hour and period minutes)
   - Port forwards as needed

### Configuring Automatic Shutdown
1. When adding or editing a machine, enable "Can be turned off remotely"
2. Set the "Turn Off Port" (typically 3001 for the client server)
3. Configure rate limiting:
   - Requests per Hour: Number of requests allowed
   - Period Minutes: Time window for rate limiting
4. The machine will automatically shut down after the configured inactivity period

### Port Forwarding
1. Add a machine to the system
2. Configure port forwards for that machine:
   - Local Port: Port on the server to listen on
   - Target Port: Port on the remote machine to forward to
3. When traffic hits the local port, the machine will be woken up if needed and traffic forwarded


### Machine Configuration
Each machine can be configured with:
- MAC Address
- IP Address
- Name and Description
- Turn-off Port (for remote shutdown)
- Request Rate Limiting:
  - Requests per Hour: Maximum requests allowed
  - Period Minutes: Time window for rate limiting
- Port Forwards:
  - Local Port: Port on the server
  - Target Port: Port on the remote machine

## How It Works

1. **Server Mode**: Runs the web interface and proxy services
2. **Client Mode**: Runs on target machines to enable remote shutdown
3. **WOL Process**: 
   - When traffic hits a configured port, the server sends a WOL packet
   - Waits for the machine to become reachable
   - Forwards traffic once the machine is up
4. **Automatic Shutdown**: 
   - Monitors request activity for each machine
   - After configured inactivity periods, sends shutdown signal
   - Uses HTTP requests to the client for shutdown

## Security Considerations

- The server should be run on a trusted network
- Access to the web interface should be restricted if exposed to the internet
- The turn-off endpoint on clients should only be accessible from the server

## Troubleshooting

### Common Issues

1. **Machine not waking up**:
   - Verify the MAC address is correct
   - Ensure WOL is enabled in the machine's BIOS/UEFI
   - Check firewall settings on the target machine
   - Verify the target machine supports WOL

2. **Proxy not working**:
   - Check that the target port is correct
   - Verify the machine is reachable after WOL
   - Ensure no firewall is blocking the connection

3. **Automatic shutdown not working**:
   - Verify the turn-off port is configured correctly
   - Ensure the client is running on the target machine
   - Check that the client can receive HTTP requests from the server

### Logs
Check the terminal output for detailed logs about:
- WOL packets sent
- Connection attempts
- Proxy activity
- Shutdown requests
- Errors and warnings

## Contributing

1. Fork the repository
2. Create a feature branch
3. Make your changes
4. Run `cargo fmt` and `cargo clippy`
5. Commit your changes
6. Push to the branch
7. Create a pull request

## License

This project is licensed under the MIT License - see the LICENSE file for details.
