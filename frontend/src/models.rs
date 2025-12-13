use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct PortForward {
    pub name: Option<String>,
    pub local_port: u16,
    pub target_port: u16,
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct NetworkInterface {
    pub name: String,
    pub ip: String,
    pub mac: String,
    pub is_up: bool,
}

#[derive(Deserialize, Debug, Clone)]
pub struct DiscoveredDevice {
    pub ip: String,
    pub mac: String,
    pub hostname: Option<String>,
}

#[derive(Debug, Deserialize, Serialize, Clone)]
pub struct Machine {
    pub name: String,
    pub mac: String,
    pub ip: String,
    pub description: Option<String>,
    pub turn_off_port: Option<u32>,
    pub can_be_turned_off: bool,
    pub inactivity_period: u32,
    pub port_forwards: Vec<PortForward>,
}

#[derive(Debug, Serialize, Clone)]
pub struct UpdateMachinePayload {
    pub mac: String,
    pub ip: String,
    pub name: String,
    pub description: Option<String>,
    pub turn_off_port: Option<u16>,
    pub can_be_turned_off: bool,
    pub inactivity_period: u32,
    pub port_forwards: Vec<PortForward>,
}

impl validator::Validate for Machine {
    fn validate(&self) -> Result<(), validator::ValidationErrors> {
        let mut errors = validator::ValidationErrors::new();

        // Add custom validation logic here if needed
        // For now, we'll just return Ok
        if self.name.is_empty() {
            errors.add("name", validator::ValidationError::new("Name is required"));
        }
        let ip = self.ip.parse::<std::net::IpAddr>();

        if ip.is_err() {
            errors.add("ip", validator::ValidationError::new("Invalid IP address"));
        }

        // check if turn_off_port is Some and in range 1-65535
        if let Some(port) = self.turn_off_port
            && (0 == port || port > 65535)
        {
            errors.add(
                "turn_off_port",
                validator::ValidationError::new("Port must be between 1 and 65535"),
            );
        }

        if self.mac.is_empty() {
            errors.add(
                "mac",
                validator::ValidationError::new("MAC address is required"),
            );
        }
        let is_valid_mac = self
            .mac
            .chars()
            .filter(|c| c.is_ascii_hexdigit() || *c == ':' || *c == '-')
            .count()
            == self.mac.len()
            && (self.mac.len() == 17 || self.mac.len() == 12);

        if !is_valid_mac {
            errors.add(
                "mac",
                validator::ValidationError::new("Invalid MAC address"),
            );
        }

        if errors.is_empty() {
            Ok(())
        } else {
            Err(errors)
        }
    }
}
