/// NPDU routing table and forwarding logic.
use std::collections::HashMap;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct PortId(pub u32);

#[derive(Debug, Clone)]
pub enum RoutingDecision {
    LocalDeliver,
    Forward {
        next_hop: bacnet_types::NetworkAddress,
        decrement_hop: bool,
    },
    Broadcast {
        networks: Vec<u16>,
    },
    Drop(DropReason),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DropReason {
    HopCountExceeded,
    NoRoute,
    LoopDetected,
}

pub struct NpduRouter {
    routing_table: HashMap<u16, (PortId, u8)>, // network -> (port, hop_count)
    local_network: u16,
}

impl NpduRouter {
    pub fn new(local_network: u16) -> Self {
        Self {
            routing_table: HashMap::new(),
            local_network,
        }
    }

    pub fn add_route(&mut self, network: u16, port: PortId, hop_count: u8) {
        self.routing_table.insert(network, (port, hop_count));
    }

    pub fn route(
        &self,
        npdu: &bacnet_codec::npdu::Npdu,
        _incoming_port: PortId,
    ) -> Vec<(PortId, RoutingDecision)> {
        if let Some(dst) = &npdu.destination {
            if dst.network == 0xFFFF {
                // Global broadcast
                return self
                    .routing_table
                    .iter()
                    .map(|(_, (port, _))| (*port, RoutingDecision::Broadcast { networks: vec![] }))
                    .collect();
            }
            if dst.network == self.local_network || dst.network == 0 {
                return vec![(PortId(0), RoutingDecision::LocalDeliver)];
            }
            if let Some(hop) = npdu.hop_count {
                if hop == 0 {
                    return vec![(
                        PortId(0),
                        RoutingDecision::Drop(DropReason::HopCountExceeded),
                    )];
                }
            }
            if let Some((port, _)) = self.routing_table.get(&dst.network) {
                let next_hop = bacnet_types::NetworkAddress {
                    network_number: dst.network,
                    mac: bacnet_types::MacAddr::MsTP(dst.mac.first().copied().unwrap_or(0)),
                };
                return vec![(
                    *port,
                    RoutingDecision::Forward {
                        next_hop,
                        decrement_hop: true,
                    },
                )];
            }
            return vec![(PortId(0), RoutingDecision::Drop(DropReason::NoRoute))];
        }
        // No destination specifier — local delivery
        vec![(PortId(0), RoutingDecision::LocalDeliver)]
    }
}
