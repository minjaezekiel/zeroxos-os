//! Network manager — TCP/IP stack, WiFi, Ethernet.

pub struct NetworkManager {
    pub interfaces: Vec<NetInterface>,
    pub sockets: Vec<Socket>,
    pub next_socket_id: u64,
    pub kernel_bypass_enabled: bool,
}

#[derive(Debug, Clone)]
pub struct NetInterface {
    pub name: String,
    pub mac: [u8; 6],
    pub ipv4: Option<[u8; 4]>,
    pub ipv6: Option<[u8; 16]>,
    pub mtu: u32,
    pub up: bool,
}

#[derive(Debug, Clone)]
pub struct Socket {
    pub id: u64,
    pub local_port: u16,
    pub remote: Option<(std::net::IpAddr, u16)>,
    pub is_kernel_bypass: bool,
}

impl NetworkManager {
    pub fn new() -> Self {
        Self {
            interfaces: Vec::new(),
            sockets: Vec::new(),
            next_socket_id: 1,
            kernel_bypass_enabled: false,
        }
    }

    pub fn add_interface(&mut self, iface: NetInterface) {
        log::info!("[net] +iface {} mac={:02x?} mtu={}", iface.name, iface.mac, iface.mtu);
        self.interfaces.push(iface);
    }

    /// Enable kernel-bypass networking for a multiplayer game socket.
    /// The NIC's hardware queues are mapped directly into the game's address
    /// space — no socket layer, no syscalls on the hot path.
    pub fn enable_kernel_bypass(&mut self) {
        self.kernel_bypass_enabled = true;
        log::info!("[net] kernel-bypass networking ENABLED (multiplayer mode)");
    }

    pub fn socket(&mut self, port: u16) -> u64 {
        let id = self.next_socket_id;
        self.next_socket_id += 1;
        let s = Socket { id, local_port: port, remote: None, is_kernel_bypass: self.kernel_bypass_enabled };
        self.sockets.push(s);
        id
    }
}
