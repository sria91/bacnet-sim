# BACnet Simulator in Rust — Comprehensive Plan

## Table of Contents

1. [Overview & Goals](#1-overview--goals)
2. [BACnet Protocol Primer](#2-bacnet-protocol-primer)
3. [Architecture](#3-architecture)
4. [Crate & Module Layout](#4-crate--module-layout)
5. [Core Data Models](#5-core-data-models)
6. [Transport Layer Implementations](#6-transport-layer-implementations)
7. [BACnet Stack Implementation](#7-bacnet-stack-implementation)
8. [Simulation Engine](#8-simulation-engine)
9. [Scalability Strategy (Millions of Objects)](#9-scalability-strategy-millions-of-objects)
10. [Configuration & Scripting](#10-configuration--scripting)
11. [Observability & Management API](#11-observability--management-api)
12. [Comprehensive Test Plan](#12-comprehensive-test-plan)
13. [Implementation Phases & Milestones](#13-implementation-phases--milestones)
14. [Dependency Map](#14-dependency-map)
15. [Performance Targets](#15-performance-targets)

---

## 1. Overview & Goals

### Purpose

A high-fidelity BACnet simulator capable of:

- **Millions of BACnet objects** (sensors, actuators, schedules, etc.) across
- **Thousands of BACnet devices** exposed simultaneously over
- **Three transports**: BACnet/IP (Annex J), BACnet MS/TP (Clause 9), and BACnet/SC (Addendum bj)

### Design Pillars

| Pillar | Strategy |
|---|---|
| **Correctness** | Full ASHRAE 135-2020 compliance for all three data link layers |
| **Scale** | Async-first (Tokio), sharded state, zero-copy serialization |
| **Fidelity** | Realistic property value evolution, COV subscriptions, alarm/event state machines |
| **Testability** | Layered unit + integration + protocol conformance + chaos tests |
| **Operability** | REST/gRPC management API, Prometheus metrics, structured logs |

---

## 2. BACnet Protocol Primer

### Object Model

Every BACnet device exposes a tree of **Objects** identified by `(ObjectType, ObjectIdentifier)`.
Each object has **Properties** (e.g., `Present_Value`, `Object_Name`, `Status_Flags`).

Common object types for sensor simulation:

| Object Type | Use Case |
|---|---|
| Analog Input (AI) | Temperature, pressure, flow sensors |
| Analog Output (AO) | Setpoints, valve positions |
| Analog Value (AV) | Calculated quantities |
| Binary Input (BI) | On/Off digital sensors |
| Binary Output (BO) | Relays, switches |
| Binary Value (BV) | Computed binary states |
| Multi-State Input (MSI) | Multi-position switches |
| Device | The device itself (mandatory) |
| Notification Class | Alarm routing |
| Schedule | Time-based control |

### Services Needed

**Confirmed Services** (require ACK):
- `ReadProperty`, `ReadPropertyMultiple`
- `WriteProperty`, `WritePropertyMultiple`
- `SubscribeCOV`, `SubscribeCOVProperty`
- `Who-Has` / `I-Have`
- `AddListElement`, `RemoveListElement`
- `AcknowledgeAlarm`, `GetAlarmSummary`
- `ReadRange` (for trend logs)

**Unconfirmed Services**:
- `Who-Is` / `I-Am`
- `UnconfirmedCOVNotification`
- `UnconfirmedEventNotification`
- `TimeSynchronization`

### PDU Types

```
BVLL (BACnet/IP framing)
 └── NPDU (Network layer — routing, priority, hop count)
      └── APDU
           ├── BACnet-Confirmed-Request-PDU
           ├── BACnet-Unconfirmed-Request-PDU
           ├── BACnet-SimpleACK-PDU
           ├── BACnet-ComplexACK-PDU
           ├── BACnet-SegmentACK-PDU
           ├── BACnet-Error-PDU
           ├── BACnet-Reject-PDU
           └── BACnet-Abort-PDU
```

---

## 3. Architecture

```
┌─────────────────────────────────────────────────────────────────────────┐
│                        BACnet Simulator Process                          │
│                                                                          │
│  ┌──────────────┐  ┌──────────────┐  ┌────────────────────────────────┐│
│  │  BACnet/IP   │  │  BACnet/SC   │  │       BACnet MS/TP             ││
│  │  Transport   │  │  (WebSocket) │  │  (Serial/Virtual Bridge)       ││
│  │  (UDP/BVLL)  │  │  (TLS+SCRAM) │  │  (Master/Slave Token Passing)  ││
│  └──────┬───────┘  └──────┬───────┘  └──────────────┬─────────────────┘│
│         │                 │                          │                   │
│  ┌──────▼─────────────────▼──────────────────────────▼─────────────────┐│
│  │                     Transport Adapter Layer                          ││
│  │    (normalizes received frames to InboundFrame; outbound vice versa) ││
│  └──────────────────────────────┬───────────────────────────────────────┘│
│                                 │                                         │
│  ┌──────────────────────────────▼───────────────────────────────────────┐│
│  │                         NPDU Router                                  ││
│  │  (hop-count, broadcast/unicast routing across transport boundaries)  ││
│  └──────────────────────────────┬───────────────────────────────────────┘│
│                                 │                                         │
│  ┌──────────────────────────────▼───────────────────────────────────────┐│
│  │                        APDU Dispatcher                               ││
│  │  (invoke-id tracking, segmentation reassembly, timeout/retry,        ││
│  │   routes to per-device virtual handler)                              ││
│  └──────────────────────────────┬───────────────────────────────────────┘│
│                                 │                                         │
│  ┌──────────────────────────────▼───────────────────────────────────────┐│
│  │                      Device Handler Pool                             ││
│  │  ┌──────────────┐  ┌──────────────┐  ┌──────────────┐              ││
│  │  │  Device 1    │  │  Device 2    │  │  Device N    │  ...         ││
│  │  │  (shard 0)   │  │  (shard 1)   │  │  (shard K)   │              ││
│  │  └──────┬───────┘  └──────────────┘  └──────────────┘              ││
│  └─────────┼─────────────────────────────────────────────────────────────┘│
│            │                                                              │
│  ┌─────────▼─────────────────────────────────────────────────────────────┐│
│  │                    Object Store (Sharded)                             ││
│  │  ┌────────────────────────────────────────────────────────────────┐  ││
│  │  │  Shard 0: Arc<RwLock<HashMap<ObjectId, BacnetObject>>>         │  ││
│  │  │  Shard 1: ...                                                  │  ││
│  │  │  Shard N: ...  (DashMap or custom sharding)                    │  ││
│  │  └────────────────────────────────────────────────────────────────┘  ││
│  └───────────────────────────────────────────────────────────────────────┘│
│                                                                            │
│  ┌──────────────────────┐  ┌────────────────────┐  ┌──────────────────┐  │
│  │  Simulation Engine   │  │  COV Engine         │  │  Alarm Engine    │  │
│  │  (value tick loop)   │  │  (subscription mgr) │  │  (state machine) │  │
│  └──────────────────────┘  └────────────────────┘  └──────────────────┘  │
│                                                                            │
│  ┌──────────────────────────────────────────────────────────────────────┐  │
│  │         Management API (REST/gRPC) + Prometheus /metrics             │  │
│  └──────────────────────────────────────────────────────────────────────┘  │
└────────────────────────────────────────────────────────────────────────────┘
```

---

## 4. Crate & Module Layout

```
bacnet-sim/
├── Cargo.toml                    # workspace
├── crates/
│   ├── bacnet-types/             # Pure data types, enums, encoding — no I/O
│   │   ├── src/
│   │   │   ├── lib.rs
│   │   │   ├── object_types.rs   # ObjectType enum
│   │   │   ├── property_id.rs    # PropertyIdentifier enum
│   │   │   ├── application_tags.rs # BACnet application tag encoding
│   │   │   ├── encoding/
│   │   │   │   ├── asn1.rs       # BACnet ASN.1 encoder/decoder
│   │   │   │   ├── context.rs    # Context-tagged encoding helpers
│   │   │   │   └── application.rs
│   │   │   └── error.rs
│   │
│   ├── bacnet-codec/             # PDU serialization (zero-copy via bytes::Bytes)
│   │   ├── src/
│   │   │   ├── apdu/
│   │   │   │   ├── confirmed.rs
│   │   │   │   ├── unconfirmed.rs
│   │   │   │   ├── ack.rs
│   │   │   │   ├── error.rs
│   │   │   │   └── segmentation.rs
│   │   │   ├── npdu.rs
│   │   │   ├── bvll.rs           # BACnet/IP BVLL
│   │   │   ├── mstp.rs           # MS/TP frame codec
│   │   │   └── sc.rs             # BACnet/SC WebSocket framing
│   │
│   ├── bacnet-transport/         # Async transport implementations
│   │   ├── src/
│   │   │   ├── ip.rs             # UDP BACnet/IP socket
│   │   │   ├── bbmd.rs           # BBMD support (foreign device, distribution)
│   │   │   ├── mstp/
│   │   │   │   ├── mod.rs
│   │   │   │   ├── master.rs     # Token-passing master state machine
│   │   │   │   ├── slave.rs
│   │   │   │   └── virtual_link.rs # In-process virtual MS/TP bus
│   │   │   └── sc/
│   │   │       ├── mod.rs
│   │   │       ├── node.rs       # SC node (hub/direct connect)
│   │   │       ├── tls.rs        # TLS/SCRAM-SHA-256 auth
│   │   │       └── hub.rs        # SC hub implementation
│   │
│   ├── bacnet-stack/             # Core BACnet services
│   │   ├── src/
│   │   │   ├── router.rs         # NPDU routing
│   │   │   ├── dispatcher.rs     # APDU dispatch, invoke-id, retry, segments
│   │   │   ├── services/
│   │   │   │   ├── who_is.rs
│   │   │   │   ├── read_property.rs
│   │   │   │   ├── read_property_multiple.rs
│   │   │   │   ├── write_property.rs
│   │   │   │   ├── subscribe_cov.rs
│   │   │   │   ├── alarm.rs
│   │   │   │   ├── schedule.rs
│   │   │   │   └── time_sync.rs
│   │   │   └── segmentation.rs
│   │
│   ├── bacnet-object/            # Object/property model
│   │   ├── src/
│   │   │   ├── store.rs          # Sharded object store
│   │   │   ├── device.rs         # Device object
│   │   │   ├── analog_input.rs
│   │   │   ├── analog_output.rs
│   │   │   ├── analog_value.rs
│   │   │   ├── binary_input.rs
│   │   │   ├── binary_output.rs
│   │   │   ├── multistate.rs
│   │   │   ├── notification_class.rs
│   │   │   ├── schedule.rs
│   │   │   ├── trend_log.rs
│   │   │   └── property.rs       # Property enum + access rules (required/optional/writable)
│   │
│   ├── bacnet-sim-engine/        # Simulation logic
│   │   ├── src/
│   │   │   ├── engine.rs         # Tick loop, coordinates all sub-engines
│   │   │   ├── value_model.rs    # Sine, random walk, step, constant, expression
│   │   │   ├── cov_engine.rs     # COV subscription tracking + notification dispatch
│   │   │   ├── alarm_engine.rs   # Intrinsic/algorithmic reporting state machines
│   │   │   ├── schedule_engine.rs
│   │   │   └── scenario.rs       # Scenario scripting (load from file)
│   │
│   ├── bacnet-config/            # Config loading (TOML/YAML/JSON)
│   │   └── src/
│   │       ├── topology.rs       # Device/object topology definition
│   │       └── profile.rs        # Device profile templates
│   │
│   ├── bacnet-api/               # Management REST/gRPC API
│   │   └── src/
│   │       ├── rest.rs           # Axum HTTP server
│   │       ├── grpc.rs           # Tonic gRPC server
│   │       └── metrics.rs        # Prometheus exporter
│   │
│   └── bacnet-sim/               # Binary: ties everything together
│       └── src/
│           └── main.rs
│
├── tests/                        # Integration & conformance tests
│   ├── integration/
│   ├── conformance/
│   ├── load/
│   └── chaos/
│
└── benches/                      # Criterion benchmarks
```

---

## 5. Core Data Models

### Object Identity

```rust
// bacnet-types/src/lib.rs

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct ObjectId {
    pub object_type: ObjectType,
    pub instance:    u32,    // 22-bit max (0..=0x3FFFFF)
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct DeviceId(pub u32);   // 22-bit device instance number

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct NetworkAddress {
    pub network_number: u16,     // 0 = local
    pub mac: MacAddr,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum MacAddr {
    Ip(std::net::SocketAddrV4),
    MsTP(u8),                    // 0–127 master, 128–255 slave
    Sc(ScNodeId),
}
```

### BACnet Property Value

```rust
// bacnet-types/src/property_value.rs

#[derive(Debug, Clone, PartialEq)]
pub enum PropertyValue {
    Null,
    Boolean(bool),
    Unsigned(u32),
    Integer(i32),
    Real(f32),
    Double(f64),
    OctetString(bytes::Bytes),
    CharacterString(String),
    BitString(BitString),
    Enumerated(u32),
    Date(BacnetDate),
    Time(BacnetTime),
    ObjectId(ObjectId),
    Array(Vec<PropertyValue>),
    List(Vec<PropertyValue>),
    Any(bytes::Bytes),           // raw encoded, for pass-through
}
```

### BACnet Object Trait

```rust
// bacnet-object/src/property.rs

pub trait BacnetObject: Send + Sync {
    fn object_id(&self) -> ObjectId;
    fn device_id(&self) -> DeviceId;

    fn read_property(
        &self,
        property_id: PropertyIdentifier,
        array_index: Option<u32>,
    ) -> Result<PropertyValue, BacnetError>;

    fn write_property(
        &mut self,
        property_id: PropertyIdentifier,
        array_index: Option<u32>,
        value: PropertyValue,
        priority: Option<u8>,
    ) -> Result<(), BacnetError>;

    /// Called every simulation tick
    fn tick(&mut self, now: SystemTime, delta: Duration);

    /// Returns properties that have changed since `since` (for COV)
    fn changed_since(&self, since: Instant) -> Vec<PropertyIdentifier>;
}
```

### Analog Input (Example Full Object)

```rust
// bacnet-object/src/analog_input.rs

pub struct AnalogInput {
    pub device_id:           DeviceId,
    pub object_id:           ObjectId,
    pub object_name:         String,
    pub description:         String,
    pub present_value:       f32,
    pub status_flags:        StatusFlags,       // [in-alarm, fault, overridden, out-of-service]
    pub event_state:         EventState,
    pub reliability:         Reliability,
    pub out_of_service:      bool,
    pub units:               EngineeringUnits,
    pub min_present_value:   Option<f32>,
    pub max_present_value:   Option<f32>,
    pub cov_increment:       f32,
    pub time_delay:          u32,
    pub notification_class:  Option<u32>,
    pub high_limit:          Option<f32>,
    pub low_limit:           Option<f32>,
    pub deadband:            f32,
    pub limit_enable:        LimitEnable,
    pub event_enable:        EventTransitionBits,
    pub acked_transitions:   EventTransitionBits,
    pub profile_name:        Option<String>,

    // Simulator internals
    pub(crate) value_model:  Box<dyn ValueModel>,
    pub(crate) last_cov_val: f32,
    pub(crate) last_changed: Instant,
}
```

---

## 6. Transport Layer Implementations

### 6a. BACnet/IP (UDP + BVLL)

```
BVLL Frame Types the simulator must handle:
  0x01 BVLC-Result
  0x04 Write-Broadcast-Distribution-Table
  0x05 Read-Broadcast-Distribution-Table
  0x06 Read-Broadcast-Distribution-Table-Ack
  0x0A Forwarded-NPDU
  0x0B Register-Foreign-Device
  0x0C Read-Foreign-Device-Table
  0x0D Read-Foreign-Device-Table-Ack
  0x0E Delete-Foreign-Device-Table-Entry
  0x0F Distribute-Broadcast-To-Network
  0x10 Original-Unicast-NPDU
  0x11 Original-Broadcast-NPDU
```

**Key implementation concerns:**
- One UDP socket per simulated BACnet/IP device OR multiplexed on a single socket with virtual-device dispatch
- For millions of objects across thousands of devices: multiplex on a small number of sockets, dispatch by Device Instance in NPDU `destination`
- BBMD implementation for cross-subnet broadcast support

```rust
// bacnet-transport/src/ip.rs (outline)

pub struct BacnetIpTransport {
    socket:      Arc<UdpSocket>,          // Tokio async UDP
    bbmd_table:  Arc<RwLock<BdtTable>>,
    fdt:         Arc<RwLock<FdtTable>>,
    tx:          mpsc::Sender<OutboundPacket>,
    rx:          broadcast::Sender<InboundFrame>,
}

impl BacnetIpTransport {
    pub async fn new(bind_addr: SocketAddr) -> Result<Self>;
    pub async fn run(self);              // recv loop + send loop
    async fn handle_bvll(&self, buf: &[u8], src: SocketAddr);
    async fn send_unicast(&self, npdu: &[u8], dst: SocketAddr);
    async fn send_broadcast(&self, npdu: &[u8]);
    async fn forward_to_bbmd(&self, npdu: &[u8], dst_bbmd: SocketAddr);
}
```

### 6b. BACnet MS/TP

MS/TP is a token-passing bus with one master and up to 127 additional masters + 128 slaves.
For simulation, we implement an **in-process virtual bus**.

```
Frame Types:
  0x00 Token
  0x01 Poll-For-Master
  0x02 Reply-To-Poll-For-Master
  0x03 Test-Request
  0x04 Test-Response
  0x05 BACnet-Data-Expecting-Reply
  0x06 BACnet-Data-Not-Expecting-Reply
  0x07 Reply-Postponed
```

**Virtual Bus Architecture:**

```rust
// bacnet-transport/src/mstp/virtual_link.rs

/// Shared bus: all "nodes" share this channel pair
pub struct VirtualMstpBus {
    // mpsc broadcast — every node sees every frame
    tx: broadcast::Sender<MstpFrame>,
}

pub struct VirtualMstpNode {
    address: u8,
    bus_tx: broadcast::Sender<MstpFrame>,
    bus_rx: broadcast::Receiver<MstpFrame>,
    // Master state machine fields
    this_station:    u8,
    next_station:    u8,
    token_count:     u8,
    retry_count:     u8,
    state:           MasterState,
}

pub enum MasterState {
    Initialize,
    Idle,
    UseToken,
    WaitForReply,
    PassToken,
    DoneWithToken,
    PollForMaster,
    AnswerTestRequest,
    NoToken,
}
```

**For physical MS/TP** (real serial ports):
- Use `tokio-serial` + `serialport` crate
- CRC-16 (IBM/ANSI) frame check

### 6c. BACnet/SC (Secure Connect)

BACnet/SC (Addendum bj to ASHRAE 135-2020) uses:
- **WebSocket** transport (RFC 6455)
- **TLS 1.2+** with mutual authentication
- **SCRAM-SHA-256** or X.509 certificate auth
- Encapsulates BACnet NDPUs in SC frames

```
SC Frame Header:
  BVLC Function (1 byte)
  Control Flags (1 byte)
  Message ID (2 bytes)
  Originating Virtual Address (optional)
  Destination Virtual Address (optional)
  Destination Options (optional)
  Data Options (optional)
  Payload
```

**Hub vs. Direct-Connect:**
- Hub: central WebSocket server; all nodes connect to it
- Direct-Connect: node-to-node WebSocket connections

```rust
// bacnet-transport/src/sc/hub.rs

pub struct ScHub {
    listener:     TcpListener,
    tls_acceptor: Arc<TlsAcceptor>,
    nodes:        Arc<DashMap<ScNodeId, ScNodeConn>>,
    routing_tx:   mpsc::Sender<RoutedScFrame>,
}

pub struct ScNode {
    node_id:    ScNodeId,     // UUID
    vmac:       [u8; 6],      // Virtual MAC address
    hub_url:    Url,
    tls_conn:   Arc<TlsConnector>,
    ws_stream:  Option<WebSocketStream<MaybeTlsStream<TcpStream>>>,
}
```

---

## 7. BACnet Stack Implementation

### NPDU Router

```rust
// bacnet-stack/src/router.rs

pub struct NpduRouter {
    routing_table: HashMap<u16, NetworkPort>,   // network_number -> port
    local_network: u16,
}

impl NpduRouter {
    pub fn route(&self, npdu: &Npdu, incoming_port: PortId)
        -> Vec<(PortId, RoutingDecision)>;
    pub fn add_route(&mut self, network: u16, port: PortId, hop_count: u8);
}

pub enum RoutingDecision {
    LocalDeliver,
    Forward { next_hop: NetworkAddress, decrement_hop: bool },
    Broadcast { networks: Vec<u16> },
    Drop(DropReason),
}
```

### APDU Dispatcher

```rust
// bacnet-stack/src/dispatcher.rs

pub struct ApduDispatcher {
    invoke_table: Arc<DashMap<(NetworkAddress, u8), PendingInvoke>>,
    segment_buf:  Arc<DashMap<(NetworkAddress, u8), SegmentBuffer>>,
    device_pool:  Arc<DeviceHandlerPool>,
}

struct PendingInvoke {
    service: ConfirmedServiceChoice,
    sent_at: Instant,
    retries: u8,
    tx:      oneshot::Sender<ApduResponse>,
}

impl ApduDispatcher {
    pub async fn dispatch(&self, apdu: Apdu, src: NetworkAddress);
    pub async fn send_confirmed(
        &self,
        service: ConfirmedServiceRequest,
        dst: NetworkAddress,
    ) -> Result<ComplexAck, BacnetError>;
    async fn handle_segmented(&self, seg: Segment, src: NetworkAddress);
    async fn handle_timeout(&self, key: (NetworkAddress, u8));
}
```

### Read Property Service

```rust
// bacnet-stack/src/services/read_property.rs

pub async fn handle_read_property(
    req: ReadPropertyRequest,
    store: &ObjectStore,
    device_id: DeviceId,
) -> Result<ReadPropertyAck, BacnetError> {
    let obj = store
        .get(device_id, req.object_id)
        .ok_or(BacnetError::UnknownObject)?;

    let value = obj
        .read()
        .await
        .read_property(req.property_id, req.array_index)?;

    Ok(ReadPropertyAck {
        object_id:   req.object_id,
        property_id: req.property_id,
        array_index: req.array_index,
        value,
    })
}
```

---

## 8. Simulation Engine

### Value Models

```rust
// bacnet-sim-engine/src/value_model.rs

pub trait ValueModel: Send + Sync {
    fn next(&mut self, t: f64, rng: &mut SmallRng) -> f32;
}

/// Sine wave with noise
pub struct SineModel {
    pub amplitude: f32,
    pub period_s:  f64,
    pub offset:    f32,
    pub noise_std: f32,
}

/// Random walk (Brownian motion)  
pub struct RandomWalkModel {
    pub current:    f32,
    pub step_std:   f32,
    pub min:        f32,
    pub max:        f32,
}

/// Step function driven by a schedule
pub struct StepModel {
    pub schedule: Vec<(f64, f32)>,  // (time_s, value)
}

/// Exponential approach (HVAC thermal model)
pub struct ThermalModel {
    pub setpoint:     f32,
    pub current:      f32,
    pub time_const_s: f64,
    pub ambient:      f32,
    pub noise_std:    f32,
}

/// Composed: sum of multiple models
pub struct CompositeModel(pub Vec<Box<dyn ValueModel>>);

/// Expression-based (rhai script)
pub struct ExprModel {
    pub engine: rhai::Engine,
    pub ast:    rhai::AST,
}
```

### COV Engine

```rust
// bacnet-sim-engine/src/cov_engine.rs

pub struct CovEngine {
    subscriptions: Arc<DashMap<CovSubKey, CovSubscription>>,
    notify_tx:     mpsc::Sender<CovNotification>,
}

#[derive(Hash, Eq, PartialEq, Clone)]
pub struct CovSubKey {
    pub subscriber:  NetworkAddress,
    pub process_id:  u32,
    pub object_id:   ObjectId,
    pub device_id:   DeviceId,
}

pub struct CovSubscription {
    pub confirmed:        bool,
    pub lifetime_secs:    Option<u32>,
    pub subscribed_at:    Instant,
    pub cov_increment:    Option<f32>,
    pub monitored_prop:   Option<PropertyIdentifier>,
    pub last_notified_at: Instant,
    pub last_value:       PropertyValue,
}

impl CovEngine {
    /// Called by the object tick loop with changed values
    pub async fn check_and_notify(
        &self,
        device_id: DeviceId,
        object_id: ObjectId,
        property:  PropertyIdentifier,
        new_value: &PropertyValue,
    );

    pub async fn subscribe(&self, key: CovSubKey, sub: CovSubscription);
    pub async fn unsubscribe(&self, key: &CovSubKey);

    /// Background loop: expire dead subscriptions, flush backlog
    pub async fn run(self);
}
```

### Alarm Engine

State machine per object following ASHRAE 135-2020 §13:

```rust
// bacnet-sim-engine/src/alarm_engine.rs

pub enum IntrinsicAlgorithm {
    OutOfRange { high_limit: f32, low_limit: f32, deadband: f32 },
    ChangeOfValue { cov_increment: f32 },
    CommandFailure { feedback_timeout: Duration },
    Floating { setpoint: f32, tolerance: f32 },
}

pub enum EventState {
    Normal,
    Fault,
    OffNormal,
    HighLimit,
    LowLimit,
    LifeSafetyAlarm,
}

pub struct AlarmStateMachine {
    pub current_state:     EventState,
    pub time_delay:        Duration,
    pub notification_class: u32,
    pub event_enable:      EventTransitionBits,
    pub acked_transitions: EventTransitionBits,
    pub algorithm:         IntrinsicAlgorithm,
    pub transition_time:   Option<Instant>,
}

impl AlarmStateMachine {
    pub fn evaluate(&mut self, value: &PropertyValue, now: Instant)
        -> Option<EventNotification>;
}
```

---

## 9. Scalability Strategy (Millions of Objects)

### Memory Layout

For 10 million objects across 10,000 devices (1,000 objects/device average):

| Component | Per Object | Total (10M) |
|---|---|---|
| AnalogInput (full) | ~320 bytes | ~3.2 GB |
| ObjectId index | 8 bytes | 80 MB |
| COV metadata | 64 bytes | 640 MB |
| **Estimated total** | | **~4 GB** |

**Memory optimizations:**
1. **Value model pointer** uses `Box<dyn ValueModel>` — most models are small (<64B)
2. **String interning** for `object_name`, `description` via `Arc<str>`
3. **Sparse optional fields** as `Option<T>` — absent in most objects
4. **History buffers** use ring buffers (bounded) rather than `Vec`

### Sharding Strategy

```rust
// bacnet-object/src/store.rs

const NUM_SHARDS: usize = 256;  // power-of-two for fast modulo

pub struct ObjectStore {
    shards: Vec<Arc<RwLock<ShardMap>>>,
}

type ShardMap = HashMap<(DeviceId, ObjectId), Box<dyn BacnetObject>>;

impl ObjectStore {
    fn shard_for(&self, device: DeviceId, obj: ObjectId) -> usize {
        // FxHash for speed, mix device+object bits
        let mut h = FxHasher::default();
        device.0.hash(&mut h);
        obj.instance.hash(&mut h);
        (h.finish() as usize) % NUM_SHARDS
    }

    pub fn get(&self, device: DeviceId, obj: ObjectId)
        -> Option<&Arc<RwLock<ShardMap>>>;

    /// Bulk insert: sorted by shard to minimize lock contention
    pub async fn bulk_insert(&self, objects: Vec<(DeviceId, Box<dyn BacnetObject>)>);
}
```

### Tick Loop Parallelism

```rust
// bacnet-sim-engine/src/engine.rs

pub struct SimEngine {
    store:      Arc<ObjectStore>,
    cov:        Arc<CovEngine>,
    alarm:      Arc<AlarmEngine>,
    tick_hz:    f64,            // e.g., 1.0 = 1 tick/sec per object
}

impl SimEngine {
    pub async fn run(self) {
        let mut interval = tokio::time::interval(
            Duration::from_secs_f64(1.0 / self.tick_hz)
        );
        loop {
            interval.tick().await;
            let now = Instant::now();
            // Rayon parallel tick across all shards
            self.store.shards.par_iter().for_each(|shard| {
                let mut guard = shard.write().unwrap();
                for obj in guard.values_mut() {
                    let changed = obj.tick(now);
                    if !changed.is_empty() {
                        // queue to async COV/alarm
                    }
                }
            });
        }
    }
}
```

**Key insight:** The tick loop uses **rayon** (parallel CPU work), while network I/O uses **Tokio** (async). They communicate via bounded `crossbeam` channels.

### Virtual Device Multiplexing

Rather than binding one UDP socket per device:

```
Real socket: 0.0.0.0:47808
  ↓
Inbound NPDU: has destination device-instance in DADR/SADR or broadcast
  ↓
Device Lookup Table: HashMap<DeviceId, DeviceHandler>
  ↓
Route to correct virtual device
```

This allows 10,000 devices on a single socket.  
For MS/TP: all devices on a segment share the virtual bus; the simulator handles the token for each master address in sequence.

---

## 10. Configuration & Scripting

### Topology File (TOML)

```toml
# topology.toml

[simulator]
tick_hz = 1.0
seed = 42

[[networks]]
id = 1
transport = "bacnet_ip"
bind = "0.0.0.0:47808"
bbmd = { enabled = false }

[[networks]]
id = 2
transport = "mstp"
mode = "virtual"            # or "serial" with port = "/dev/ttyS0"
baud = 76800
mac_range = [1, 127]

[[networks]]
id = 3
transport = "bacnet_sc"
hub_url = "wss://localhost:47814"
ca_cert = "certs/ca.pem"
node_cert = "certs/hub.pem"
node_key = "certs/hub.key"

# Device profile templates
[profiles.hvac_ahu]
description = "Air Handling Unit"
objects = [
  { type = "AnalogInput",  count = 12, name_prefix = "SAT",  units = "DegreesCelsius",    model = "sine",    model_params = { amplitude = 5.0, period_s = 3600, offset = 22.0 } },
  { type = "AnalogOutput", count = 4,  name_prefix = "VLV",  units = "Percent",           model = "step" },
  { type = "BinaryInput",  count = 8,  name_prefix = "STAT", model = "random_toggle",     model_params = { mean_period_s = 300 } },
]

# Instantiate devices
[[devices]]
id_range = [1000, 1999]           # 1000 devices
network = 1
profile = "hvac_ahu"
```

### Scripted Scenarios (Rhai)

```rhai
// scenarios/fire_alarm_test.rhai

let t = sim_time();

if t > 300.0 {
    // After 5 minutes, force BI-001 into alarm
    set_property(device(1001), object("BinaryInput", 1), "PresentValue", true);
    set_property(device(1001), object("BinaryInput", 1), "StatusFlags", 
                 status_flags(in_alarm: true));
}

if t > 360.0 {
    // Clear alarm
    set_property(device(1001), object("BinaryInput", 1), "PresentValue", false);
}
```

---

## 11. Observability & Management API

### Prometheus Metrics

```
# HELP bacnet_devices_total Total simulated devices
# TYPE bacnet_devices_total gauge
bacnet_devices_total{transport="bacnet_ip"} 8000
bacnet_devices_total{transport="mstp"} 1500
bacnet_devices_total{transport="sc"} 500

# HELP bacnet_objects_total Total simulated objects
bacnet_objects_total 4250000

# HELP bacnet_requests_total BACnet requests processed
bacnet_requests_total{service="ReadProperty",result="ok"} 142983
bacnet_requests_total{service="ReadProperty",result="error"} 12

# HELP bacnet_tick_duration_seconds Simulation tick duration
bacnet_tick_duration_seconds{quantile="0.99"} 0.048

# HELP bacnet_cov_notifications_total COV notifications sent
bacnet_cov_notifications_total{transport="bacnet_ip"} 98231

# HELP bacnet_active_cov_subscriptions Active COV subscriptions
bacnet_active_cov_subscriptions 2847
```

### REST Management API

```
GET  /api/v1/devices                          # List all devices
GET  /api/v1/devices/{id}                     # Device details
POST /api/v1/devices                          # Create device(s)
DELETE /api/v1/devices/{id}                   # Remove device

GET  /api/v1/devices/{id}/objects             # List objects on device
GET  /api/v1/devices/{id}/objects/{type}/{instance}
PUT  /api/v1/devices/{id}/objects/{type}/{instance}/properties/{prop}

POST /api/v1/scenarios/load                   # Load scenario file
POST /api/v1/scenarios/run
POST /api/v1/scenarios/stop

GET  /api/v1/metrics                          # Prometheus format
GET  /api/v1/health
```

---

## 12. Comprehensive Test Plan

### Test Categories

```
tests/
├── unit/              (per crate, co-located in src/ with #[cfg(test)])
├── integration/       (multi-crate, real async runtime)
├── conformance/       (protocol compliance per ASHRAE 135.1)
├── load/              (scale: millions of objects, sustained throughput)
├── chaos/             (fault injection: packet loss, slow clients, OOM)
└── fuzz/              (cargo-fuzz targets for codec + encoder)
```

---

### 12.1 Unit Tests

#### bacnet-types: Encoding Correctness

```rust
// crates/bacnet-types/src/encoding/asn1.rs

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn encode_unsigned_one_byte() {
        let mut buf = BytesMut::new();
        encode_application_unsigned(&mut buf, 42);
        assert_eq!(buf.as_ref(), &[0x21, 0x2A]);  // tag=2, len=1, value=42
    }

    #[test]
    fn encode_unsigned_two_bytes() {
        let mut buf = BytesMut::new();
        encode_application_unsigned(&mut buf, 300);
        assert_eq!(buf.as_ref(), &[0x22, 0x01, 0x2C]);
    }

    #[test]
    fn encode_decode_real_roundtrip() {
        for &val in &[0.0f32, -1.0, f32::MAX, f32::MIN_POSITIVE, 3.14159] {
            let mut buf = BytesMut::new();
            encode_application_real(&mut buf, val);
            let (decoded, _) = decode_application_real(&buf).unwrap();
            assert!((decoded - val).abs() < f32::EPSILON || decoded == val);
        }
    }

    #[test]
    fn encode_object_id() {
        let oid = ObjectId { object_type: ObjectType::AnalogInput, instance: 7 };
        let mut buf = BytesMut::new();
        encode_application_object_id(&mut buf, oid);
        // AI=0, instance=7 → 0x00_000007 as 4-byte Unsigned
        assert_eq!(buf.as_ref(), &[0xC4, 0x00, 0x00, 0x00, 0x07]);
    }

    #[test]
    fn decode_date() {
        // BACnet Date: year-1900, month, day, weekday
        let bytes = [0x09, 0x79, 0x01, 0x01, 0x05]; // 2021-01-01 Friday
        let (date, _) = decode_application_date(&bytes).unwrap();
        assert_eq!(date.year, 2021);
        assert_eq!(date.month, 1);
        assert_eq!(date.day, 1);
        assert_eq!(date.weekday, Weekday::Friday);
    }

    #[test]
    fn decode_malformed_tag_returns_error() {
        let bytes = [0xFF, 0xFF, 0xFF];
        assert!(decode_application_unsigned(&bytes).is_err());
    }

    #[test]
    fn context_tag_encoding() {
        let mut buf = BytesMut::new();
        encode_context_unsigned(&mut buf, 3, 255);
        // context tag 3, length 1, value 0xFF
        assert_eq!(buf.as_ref(), &[0x3B, 0xFF]);
    }

    #[test]
    fn bit_string_roundtrip() {
        let bits = BitString::from_bits(&[true, false, true, true, false]);
        let mut buf = BytesMut::new();
        encode_application_bitstring(&mut buf, &bits);
        let (decoded, _) = decode_application_bitstring(&buf).unwrap();
        assert_eq!(decoded.bits(), bits.bits());
    }
}
```

#### bacnet-codec: PDU Roundtrip Tests

```rust
// crates/bacnet-codec/src/apdu/confirmed.rs

#[cfg(test)]
mod tests {
    use super::*;

    fn make_read_property_req() -> ConfirmedRequest {
        ConfirmedRequest {
            segmented_accepted: true,
            more_follows:       false,
            segmented_response_accepted: true,
            max_segments:       MaxSegments::Unspecified,
            max_response:       480,
            invoke_id:          42,
            sequence_number:    None,
            proposed_window:    None,
            service:            ConfirmedServiceRequest::ReadProperty(
                ReadPropertyRequest {
                    object_id:   ObjectId { object_type: ObjectType::AnalogInput, instance: 1 },
                    property_id: PropertyIdentifier::PresentValue,
                    array_index: None,
                }
            ),
        }
    }

    #[test]
    fn confirmed_request_encode_decode_roundtrip() {
        let req = make_read_property_req();
        let mut buf = BytesMut::new();
        req.encode(&mut buf);
        let decoded = ConfirmedRequest::decode(&buf).unwrap();
        assert_eq!(req.invoke_id, decoded.invoke_id);
        match &decoded.service {
            ConfirmedServiceRequest::ReadProperty(r) => {
                assert_eq!(r.object_id.instance, 1);
                assert_eq!(r.property_id, PropertyIdentifier::PresentValue);
            }
            _ => panic!("wrong service"),
        }
    }

    #[test]
    fn complex_ack_with_array_value() {
        let ack = ComplexAck {
            invoke_id: 1,
            service:   ComplexAckService::ReadProperty(ReadPropertyAck {
                object_id:   ObjectId { object_type: ObjectType::Device, instance: 1234 },
                property_id: PropertyIdentifier::ObjectList,
                array_index: Some(0),
                value:       PropertyValue::Unsigned(42),
            }),
        };
        let mut buf = BytesMut::new();
        ack.encode(&mut buf);
        let decoded = ComplexAck::decode(&buf).unwrap();
        assert_eq!(decoded.invoke_id, 1);
    }

    #[test]
    fn error_pdu_encodes_correctly() {
        let err = ErrorPdu {
            invoke_id:    99,
            service:      ConfirmedServiceChoice::ReadProperty,
            error_class:  ErrorClass::Object,
            error_code:   ErrorCode::UnknownObject,
        };
        let mut buf = BytesMut::new();
        err.encode(&mut buf);
        assert_eq!(buf[0] & 0xF0, 0x50);  // PDU type = Error
    }

    #[test]
    fn segmented_request_flags() {
        let req = make_read_property_req();
        let mut buf = BytesMut::new();
        req.encode(&mut buf);
        // Byte 0: PDU type = 0x00, SA=1, MF=0
        assert_eq!(buf[0] & 0x08, 0x08); // SA bit set
    }

    #[test]
    fn who_is_unconfirmed_pdu() {
        let pdu = UnconfirmedRequest::WhoIs(WhoIsRequest {
            low_limit: Some(1000),
            high_limit: Some(2000),
        });
        let mut buf = BytesMut::new();
        pdu.encode(&mut buf);
        let decoded = UnconfirmedRequest::decode(&buf).unwrap();
        match decoded {
            UnconfirmedRequest::WhoIs(w) => {
                assert_eq!(w.low_limit, Some(1000));
                assert_eq!(w.high_limit, Some(2000));
            }
            _ => panic!("expected WhoIs"),
        }
    }
}
```

#### bacnet-codec: BVLL Tests

```rust
#[cfg(test)]
mod bvll_tests {
    use super::*;

    #[test]
    fn original_unicast_npdu_encode_decode() {
        let npdu_data = vec![0x01, 0x20, 0x00, 0x00];
        let frame = BvllFrame::OriginalUnicastNpdu(Bytes::from(npdu_data.clone()));
        let encoded = frame.encode();
        // Type=0x81, Function=0x0A, Length=4+4=8
        assert_eq!(encoded[0], 0x81);
        assert_eq!(encoded[1], 0x0A);
        assert_eq!(&encoded[4..], npdu_data.as_slice());
        let decoded = BvllFrame::decode(&encoded).unwrap();
        assert!(matches!(decoded, BvllFrame::OriginalUnicastNpdu(_)));
    }

    #[test]
    fn register_foreign_device() {
        let frame = BvllFrame::RegisterForeignDevice { ttl: 300 };
        let encoded = frame.encode();
        assert_eq!(encoded[1], 0x0B);
        assert_eq!(u16::from_be_bytes([encoded[4], encoded[5]]), 300);
    }

    #[test]
    fn forwarded_npdu_has_originator_address() {
        let orig = SocketAddrV4::new([192,168,1,10].into(), 47808);
        let npdu = Bytes::from_static(b"\x01\x00");
        let frame = BvllFrame::ForwardedNpdu { originating_address: orig, npdu };
        let encoded = frame.encode();
        assert_eq!(encoded[1], 0x0A);
        // Bytes 4-9: IP (4) + port (2)
        assert_eq!(&encoded[4..8], &[192, 168, 1, 10]);
        assert_eq!(u16::from_be_bytes([encoded[8], encoded[9]]), 47808);
    }
}
```

#### MS/TP Frame Tests

```rust
#[cfg(test)]
mod mstp_tests {
    use super::*;

    #[test]
    fn token_frame_encode() {
        let frame = MstpFrame {
            frame_type:  MstpFrameType::Token,
            destination: 2,
            source:      1,
            data:        Bytes::new(),
        };
        let encoded = frame.encode();
        // Preamble: 0x55, 0xFF
        assert_eq!(encoded[0], 0x55);
        assert_eq!(encoded[1], 0xFF);
        assert_eq!(encoded[2], 0x00); // Token frame type
        assert_eq!(encoded[3], 0x02); // Dst
        assert_eq!(encoded[4], 0x01); // Src
        // CRC-8 header check
        let hdr_crc = compute_crc8(&encoded[2..7]);
        assert_eq!(encoded[7], hdr_crc);
    }

    #[test]
    fn data_frame_crc16() {
        let data = b"Hello BACnet".to_vec();
        let frame = MstpFrame {
            frame_type:  MstpFrameType::BacnetDataNotExpectingReply,
            destination: 5,
            source:      1,
            data:        Bytes::from(data.clone()),
        };
        let encoded = frame.encode();
        let crc16_pos = encoded.len() - 2;
        let expected_crc = compute_crc16(&encoded[8..crc16_pos]);
        let actual_crc = u16::from_le_bytes([encoded[crc16_pos], encoded[crc16_pos+1]]);
        assert_eq!(expected_crc, actual_crc);
    }

    #[test]
    fn decode_incomplete_frame_returns_need_more() {
        let partial = vec![0x55, 0xFF, 0x00]; // only preamble + frame type, no dst
        let result = MstpFrame::decode(&partial);
        assert!(matches!(result, Err(MstpDecodeError::Incomplete)));
    }

    #[test]
    fn decode_bad_preamble_returns_error() {
        let bad = vec![0xAA, 0xBB, 0x00, 0x01, 0x02, 0x00, 0x00, 0x00];
        let result = MstpFrame::decode(&bad);
        assert!(matches!(result, Err(MstpDecodeError::BadPreamble)));
    }
}
```

---

### 12.2 Integration Tests

#### BACnet/IP: Who-Is / I-Am Discovery

```rust
// tests/integration/test_whois_iam.rs

use bacnet_sim::prelude::*;
use tokio::time::{timeout, Duration};

#[tokio::test]
async fn whois_broadcast_discovers_all_devices() {
    let sim = SimBuilder::new()
        .add_network(Network::ip("127.0.0.1:47808"))
        .add_devices(DeviceRange { ids: 1..=10, profile: "minimal" })
        .build()
        .await
        .unwrap();

    let client = BacnetIpClient::connect("127.0.0.1:47808").await.unwrap();

    let discovered = timeout(Duration::from_secs(5), async {
        client.who_is(None, None).await.unwrap()
    })
    .await
    .expect("timeout waiting for I-Am responses");

    // All 10 devices should respond
    assert_eq!(discovered.len(), 10);
    for dev in &discovered {
        assert!((1..=10).contains(&dev.device_instance));
        assert_eq!(dev.max_apdu_length_accepted, 1476);
        assert_eq!(dev.segmentation_supported, Segmentation::NoSegmentation);
    }
}

#[tokio::test]
async fn whois_with_range_filters_correctly() {
    let sim = SimBuilder::new()
        .add_network(Network::ip("127.0.0.1:47809"))
        .add_devices(DeviceRange { ids: 100..=199, profile: "minimal" })
        .build().await.unwrap();

    let client = BacnetIpClient::connect("127.0.0.1:47809").await.unwrap();
    let discovered = client.who_is(Some(150), Some(160)).await.unwrap();

    assert_eq!(discovered.len(), 11); // 150..=160
    assert!(discovered.iter().all(|d| (150..=160).contains(&d.device_instance)));
}
```

#### Read Property — All Required Properties

```rust
#[tokio::test]
async fn read_all_required_analog_input_properties() {
    let sim = SimBuilder::new()
        .add_network(Network::ip("127.0.0.1:47810"))
        .add_device(DeviceConfig {
            id: 500,
            objects: vec![ObjectConfig::analog_input(1, "SAT-01", Units::DegreesCelsius)],
        })
        .build().await.unwrap();

    let client = BacnetIpClient::connect("127.0.0.1:47810").await.unwrap();

    let required_props = vec![
        PropertyIdentifier::ObjectIdentifier,
        PropertyIdentifier::ObjectName,
        PropertyIdentifier::ObjectType,
        PropertyIdentifier::PresentValue,
        PropertyIdentifier::StatusFlags,
        PropertyIdentifier::EventState,
        PropertyIdentifier::OutOfService,
        PropertyIdentifier::Units,
    ];

    for prop in required_props {
        let result = client
            .read_property(500, ObjectId::analog_input(1), prop, None)
            .await;
        assert!(
            result.is_ok(),
            "Required property {:?} missing: {:?}", prop, result
        );
    }
}

#[tokio::test]
async fn read_nonexistent_object_returns_unknown_object_error() {
    let sim = SimBuilder::new()
        .add_network(Network::ip("127.0.0.1:47811"))
        .add_device(DeviceConfig { id: 501, objects: vec![] })
        .build().await.unwrap();

    let client = BacnetIpClient::connect("127.0.0.1:47811").await.unwrap();

    let err = client
        .read_property(501, ObjectId::analog_input(999), PropertyIdentifier::PresentValue, None)
        .await
        .unwrap_err();

    assert_eq!(err.error_class, ErrorClass::Object);
    assert_eq!(err.error_code, ErrorCode::UnknownObject);
}
```

#### Write Property Tests

```rust
#[tokio::test]
async fn write_present_value_out_of_service() {
    let sim = SimBuilder::new()
        .add_network(Network::ip("127.0.0.1:47812"))
        .add_device(DeviceConfig {
            id: 600,
            objects: vec![ObjectConfig::analog_input(1, "TEST", Units::NoUnits)],
        })
        .build().await.unwrap();

    let client = BacnetIpClient::connect("127.0.0.1:47812").await.unwrap();
    let oi = ObjectId::analog_input(1);

    // Writing PresentValue while not OOS should fail
    let err = client
        .write_property(600, oi, PropertyIdentifier::PresentValue, 
                        PropertyValue::Real(42.0), None, None)
        .await.unwrap_err();
    assert_eq!(err.error_code, ErrorCode::WriteAccessDenied);

    // Set out-of-service first
    client.write_property(600, oi, PropertyIdentifier::OutOfService,
                          PropertyValue::Boolean(true), None, None)
          .await.unwrap();

    // Now write should succeed
    client.write_property(600, oi, PropertyIdentifier::PresentValue,
                          PropertyValue::Real(42.0), None, None)
          .await.unwrap();

    let val = client.read_property(600, oi, PropertyIdentifier::PresentValue, None)
                    .await.unwrap();
    assert_eq!(val, PropertyValue::Real(42.0));
}

#[tokio::test]
async fn priority_array_write_and_relinquish() {
    let sim = SimBuilder::new()
        .add_network(Network::ip("127.0.0.1:47813"))
        .add_device(DeviceConfig {
            id: 700,
            objects: vec![ObjectConfig::analog_output(1, "AO-01", Units::Percent)],
        })
        .build().await.unwrap();

    let client = BacnetIpClient::connect("127.0.0.1:47813").await.unwrap();
    let oi = ObjectId::analog_output(1);

    // Write at priority 8
    client.write_property(700, oi, PropertyIdentifier::PresentValue,
                          PropertyValue::Real(75.0), None, Some(8))
          .await.unwrap();

    // Write at priority 5 (higher priority)
    client.write_property(700, oi, PropertyIdentifier::PresentValue,
                          PropertyValue::Real(50.0), None, Some(5))
          .await.unwrap();

    let pv = client.read_property(700, oi, PropertyIdentifier::PresentValue, None).await.unwrap();
    assert_eq!(pv, PropertyValue::Real(50.0)); // Priority 5 wins

    // Relinquish priority 5
    client.write_property(700, oi, PropertyIdentifier::PresentValue,
                          PropertyValue::Null, None, Some(5))
          .await.unwrap();

    let pv = client.read_property(700, oi, PropertyIdentifier::PresentValue, None).await.unwrap();
    assert_eq!(pv, PropertyValue::Real(75.0)); // Falls back to priority 8
}
```

#### ReadPropertyMultiple Tests

```rust
#[tokio::test]
async fn rpm_single_device_multiple_objects() {
    // Setup 100 AI objects on device 800
    let sim = SimBuilder::new()
        .add_network(Network::ip("127.0.0.1:47815"))
        .add_device(DeviceConfig {
            id: 800,
            objects: (1..=100).map(|i|
                ObjectConfig::analog_input(i, &format!("AI-{i:03}"), Units::DegreesCelsius)
            ).collect(),
        })
        .build().await.unwrap();

    let client = BacnetIpClient::connect("127.0.0.1:47815").await.unwrap();

    let specs: Vec<_> = (1..=100).map(|i| ObjectPropertySpec {
        object_id: ObjectId::analog_input(i),
        properties: vec![
            (PropertyIdentifier::PresentValue, None),
            (PropertyIdentifier::StatusFlags, None),
            (PropertyIdentifier::Units, None),
        ],
    }).collect();

    let results = client.read_property_multiple(800, specs).await.unwrap();
    assert_eq!(results.len(), 100);
    for r in &results {
        assert_eq!(r.property_results.len(), 3);
        assert!(r.property_results.iter().all(|p| p.value.is_ok()));
    }
}

#[tokio::test]
async fn rpm_all_properties_wildcard() {
    let sim = SimBuilder::new()
        .add_network(Network::ip("127.0.0.1:47816"))
        .add_device(DeviceConfig {
            id: 900,
            objects: vec![ObjectConfig::analog_input(1, "AI-WILD", Units::Kelvin)],
        })
        .build().await.unwrap();

    let client = BacnetIpClient::connect("127.0.0.1:47816").await.unwrap();

    let results = client.read_property_multiple(900, vec![
        ObjectPropertySpec {
            object_id: ObjectId::analog_input(1),
            properties: vec![(PropertyIdentifier::All, None)],
        }
    ]).await.unwrap();

    let props: Vec<_> = results[0].property_results.iter()
        .map(|r| r.property_id)
        .collect();

    // Must include all required properties
    assert!(props.contains(&PropertyIdentifier::ObjectIdentifier));
    assert!(props.contains(&PropertyIdentifier::PresentValue));
    assert!(props.contains(&PropertyIdentifier::Units));
    assert!(props.contains(&PropertyIdentifier::StatusFlags));
}
```

#### COV Subscription Tests

```rust
#[tokio::test]
async fn cov_subscription_receives_notifications() {
    let sim = SimBuilder::new()
        .add_network(Network::ip("127.0.0.1:47817"))
        .add_device(DeviceConfig {
            id: 1000,
            objects: vec![ObjectConfig::analog_input(1, "TEMP", Units::DegreesCelsius)
                .with_cov_increment(0.5)
                .with_model(ValueModel::sine(5.0, 10.0, 20.0))], // fast 10s period
        })
        .build().await.unwrap();

    let client = BacnetIpClient::connect("127.0.0.1:47817").await.unwrap();

    let mut cov_rx = client.subscribe_cov(
        CovSubscription {
            subscriber_process_id: 1,
            monitored_object:      ObjectId::analog_input(1),
            device_id:             1000,
            issue_confirmed:       false,
            lifetime:              Some(60),
            cov_increment:         None,
        }
    ).await.unwrap();

    // Wait for at least 3 notifications within 30 seconds
    let mut count = 0;
    let deadline = Instant::now() + Duration::from_secs(30);
    while Instant::now() < deadline && count < 3 {
        if let Ok(Some(notif)) = timeout(Duration::from_secs(5), cov_rx.recv()).await {
            assert_eq!(notif.monitored_object, ObjectId::analog_input(1));
            assert!(notif.list_of_values.iter().any(|v|
                v.property_id == PropertyIdentifier::PresentValue
            ));
            count += 1;
        }
    }
    assert!(count >= 3, "Expected 3 COV notifications, got {count}");
}

#[tokio::test]
async fn cov_subscription_expires() {
    let sim = SimBuilder::new()
        .add_network(Network::ip("127.0.0.1:47818"))
        .add_device(DeviceConfig {
            id: 1001,
            objects: vec![ObjectConfig::analog_input(1, "TEMP", Units::DegreesCelsius)],
        })
        .build().await.unwrap();

    let client = BacnetIpClient::connect("127.0.0.1:47818").await.unwrap();

    client.subscribe_cov(CovSubscription {
        subscriber_process_id: 2,
        monitored_object:      ObjectId::analog_input(1),
        device_id:             1001,
        issue_confirmed:       false,
        lifetime:              Some(2), // 2-second lifetime
        cov_increment:         None,
    }).await.unwrap();

    // Verify subscription is active
    let active = sim.get_cov_subscriptions(1001, ObjectId::analog_input(1)).await;
    assert!(!active.is_empty());

    // Wait for expiry
    tokio::time::sleep(Duration::from_secs(4)).await;

    let active = sim.get_cov_subscriptions(1001, ObjectId::analog_input(1)).await;
    assert!(active.is_empty());
}
```

#### Alarm / Event Tests

```rust
#[tokio::test]
async fn high_limit_alarm_transitions() {
    let sim = SimBuilder::new()
        .add_network(Network::ip("127.0.0.1:47820"))
        .add_device(DeviceConfig {
            id: 2000,
            objects: vec![
                ObjectConfig::analog_input(1, "PRESSURE", Units::PoundsPerSquareInch)
                    .with_intrinsic_reporting(IntrinsicConfig {
                        notification_class: 1,
                        high_limit: Some(100.0),
                        low_limit:  Some(0.0),
                        deadband:   5.0,
                        time_delay: 0,
                        event_enable: EventTransitionBits::all(),
                    })
                    .with_model(ValueModel::constant(50.0)),
                ObjectConfig::notification_class(1),
            ],
        })
        .build().await.unwrap();

    // Subscribe to event notifications
    let client = BacnetIpClient::connect("127.0.0.1:47820").await.unwrap();
    let mut events = client.subscribe_event_notifications(2000).await.unwrap();

    // Force value above high limit
    sim.force_value(2000, ObjectId::analog_input(1), PropertyValue::Real(110.0)).await;

    let event = timeout(Duration::from_secs(5), events.recv())
        .await.expect("no event").unwrap();

    assert_eq!(event.event_object_identifier, ObjectId::analog_input(1));
    assert_eq!(event.to_state, EventState::HighLimit);
    assert_eq!(event.from_state, EventState::Normal);
    assert!(event.event_values.iter().any(|v| v.exceeded_limit == Some(100.0)));

    // Restore value within deadband
    sim.force_value(2000, ObjectId::analog_input(1), PropertyValue::Real(93.0)).await;

    let event = timeout(Duration::from_secs(5), events.recv())
        .await.expect("no event").unwrap();

    assert_eq!(event.to_state, EventState::Normal);
}
```

#### MS/TP Integration Tests

```rust
#[tokio::test]
async fn mstp_virtual_bus_token_passing() {
    let bus = VirtualMstpBus::new();
    
    // Create 5 master nodes
    let masters: Vec<_> = (0..5u8).map(|i| {
        VirtualMstpMaster::join(&bus, i, 4) // address i, max_master=4
    }).collect();

    // Let the bus run for 1 second
    tokio::time::sleep(Duration::from_secs(1)).await;

    // All masters should have received the token at least once
    for (i, m) in masters.iter().enumerate() {
        let token_count = m.token_received_count().await;
        assert!(token_count > 0, "Master {i} never received token");
    }
}

#[tokio::test]
async fn mstp_data_frame_delivery() {
    let bus = VirtualMstpBus::new();
    let master0 = VirtualMstpMaster::join(&bus, 0, 1);
    let master1 = VirtualMstpMaster::join(&bus, 1, 1);

    // Wait for token
    master0.wait_for_token(Duration::from_secs(2)).await.unwrap();

    let test_data = Bytes::from_static(b"\x01\x00\x00\xFF"); // simple NPDU

    // Send data from master 0 to master 1
    master0.send_data(1, test_data.clone(), true).await.unwrap();

    // Master 1 should receive it
    let received = timeout(Duration::from_secs(2), master1.recv_data())
        .await.unwrap().unwrap();

    assert_eq!(received.source, 0);
    assert_eq!(received.data, test_data);
}
```

#### BACnet/SC Tests

```rust
#[tokio::test]
async fn sc_node_connects_to_hub_and_registers() {
    let hub = ScHub::start("127.0.0.1:47830", test_tls_config()).await.unwrap();

    let node = ScNode::connect(
        "wss://127.0.0.1:47830",
        ScNodeId::random(),
        [0xDE, 0xAD, 0xBE, 0xEF, 0x00, 0x01],
        test_tls_config(),
    ).await.unwrap();

    assert_eq!(node.connection_state(), ScConnectionState::Connected);
    assert_eq!(hub.connected_nodes().await.len(), 1);
}

#[tokio::test]
async fn sc_npdu_routes_between_nodes() {
    let hub = ScHub::start("127.0.0.1:47831", test_tls_config()).await.unwrap();
    let node_a = ScNode::connect("wss://127.0.0.1:47831", ...).await.unwrap();
    let node_b = ScNode::connect("wss://127.0.0.1:47831", ...).await.unwrap();

    let test_npdu = Bytes::from_static(b"\x01\x00\x00\x00");
    node_a.send_npdu(node_b.vmac(), test_npdu.clone()).await.unwrap();

    let received = timeout(Duration::from_secs(2), node_b.recv_npdu())
        .await.unwrap().unwrap();

    assert_eq!(received.data, test_npdu);
    assert_eq!(received.originating_vmac, Some(node_a.vmac()));
}
```

#### Segmentation Tests

```rust
#[tokio::test]
async fn large_rpm_response_is_segmented_and_reassembled() {
    // Create a device with enough objects that RPM response > max APDU (1476 bytes)
    let sim = SimBuilder::new()
        .add_network(Network::ip("127.0.0.1:47840"))
        .add_device(DeviceConfig {
            id: 3000,
            objects: (1..=500).map(|i|
                ObjectConfig::analog_input(i, &format!("LONG_NAME_AI_{i:04}"), Units::Kelvin)
            ).collect(),
        })
        .build().await.unwrap();

    let client = BacnetIpClient::connect("127.0.0.1:47840").await.unwrap();

    // Read all objects' Object_Name — this will exceed max APDU
    let specs: Vec<_> = (1..=500).map(|i| ObjectPropertySpec {
        object_id: ObjectId::analog_input(i),
        properties: vec![(PropertyIdentifier::ObjectName, None)],
    }).collect();

    let results = client.read_property_multiple(3000, specs).await.unwrap();
    assert_eq!(results.len(), 500);

    // Verify data integrity across segment boundaries
    for (i, r) in results.iter().enumerate() {
        let name = r.property_results[0].value.as_ref().unwrap();
        assert_eq!(name, &PropertyValue::CharacterString(format!("LONG_NAME_AI_{:04}", i+1)));
    }
}

#[tokio::test]
async fn segmented_request_with_window_size_negotiation() {
    // Tests that window size is respected and segment ACKs are sent correctly
    let sim = SimBuilder::new()
        .add_network(Network::ip("127.0.0.1:47841"))
        .add_device(DeviceConfig { id: 3001, objects: vec![
            ObjectConfig::analog_output(1, "AO-SEG", Units::Percent)
        ]})
        .build().await.unwrap();

    let client = BacnetIpClient::builder("127.0.0.1:47841")
        .max_apdu(128)           // force segmentation at 128 bytes
        .window_size(4)
        .build().await.unwrap();

    // WriteProperty with large CharacterString (triggers segmented request)
    let long_string = "A".repeat(500);
    let result = client.write_property(
        3001, ObjectId::analog_output(1),
        PropertyIdentifier::Description,
        PropertyValue::CharacterString(long_string.clone()),
        None, None,
    ).await;

    assert!(result.is_ok());

    // Read back to verify
    let readback = client.read_property(3001, ObjectId::analog_output(1),
                                        PropertyIdentifier::Description, None).await.unwrap();
    assert_eq!(readback, PropertyValue::CharacterString(long_string));
}
```

---

### 12.3 Conformance Tests

Based on ASHRAE 135.1 standard conformance test procedures.

```rust
// tests/conformance/mod.rs

/// BACnet Protocol Implementation Conformance Statement (PICS) checks
pub struct ConformanceSuite {
    client:  BacnetIpClient,
    device:  DeviceId,
}

impl ConformanceSuite {
    /// 9.1 — Device Object Required Properties
    pub async fn test_device_object_required_properties(&self) {
        let required = vec![
            PropertyIdentifier::ObjectIdentifier,
            PropertyIdentifier::ObjectName,
            PropertyIdentifier::ObjectType,
            PropertyIdentifier::SystemStatus,
            PropertyIdentifier::VendorName,
            PropertyIdentifier::VendorIdentifier,
            PropertyIdentifier::ModelName,
            PropertyIdentifier::FirmwareRevision,
            PropertyIdentifier::ApplicationSoftwareVersion,
            PropertyIdentifier::ProtocolVersion,
            PropertyIdentifier::ProtocolRevision,
            PropertyIdentifier::ProtocolServicesSupported,
            PropertyIdentifier::ProtocolObjectTypesSupported,
            PropertyIdentifier::ObjectList,
            PropertyIdentifier::MaxApduLengthAccepted,
            PropertyIdentifier::SegmentationSupported,
            PropertyIdentifier::ApduTimeout,
            PropertyIdentifier::NumberOfApduRetries,
            PropertyIdentifier::MaxMaster,     // MS/TP only
            PropertyIdentifier::DatabaseRevision,
        ];
        for prop in required {
            let result = self.client
                .read_property(self.device, ObjectId::device(self.device), prop, None)
                .await;
            assert!(result.is_ok(), "CONFORMANCE: Required Device property {prop:?} missing");
        }
    }

    /// 12.11 — SubscribeCOV Execute
    pub async fn test_subscribecov_execute(&self, monitored_object: ObjectId) {
        // Step 1: Subscribe
        let sub = CovSubscription {
            subscriber_process_id: 42,
            monitored_object,
            device_id: self.device,
            issue_confirmed: true,
            lifetime: Some(60),
            cov_increment: None,
        };
        self.client.subscribe_cov(sub).await.expect("SubscribeCOV failed");

        // Step 2: Receive initial notification
        // (simulator must send current values immediately on subscription)
        let init = timeout(Duration::from_secs(3),
                           self.client.next_cov_notification())
            .await.expect("No initial COV notification");
        assert_eq!(init.monitored_object, monitored_object);
        assert!(!init.list_of_values.is_empty());

        // Step 3: Unsubscribe (lifetime=0)
        self.client.subscribe_cov(CovSubscription {
            lifetime: Some(0), ..sub
        }).await.expect("Unsubscribe failed");
    }

    /// 13.3.6 — Error Response Codes
    pub async fn test_error_response_unknown_property(&self) {
        let err = self.client.read_property(
            self.device,
            ObjectId::device(self.device),
            PropertyIdentifier::Unknown(9999),
            None,
        ).await.unwrap_err();
        assert_eq!(err.error_class, ErrorClass::Property);
        assert_eq!(err.error_code, ErrorCode::UnknownProperty);
    }

    /// 16.1 — Who-Is / I-Am
    pub async fn test_who_is_i_am_conformance(&self) {
        // Global broadcast
        let responses = self.client.who_is(None, None).await.unwrap();
        assert!(!responses.is_empty());

        // Device must not respond to out-of-range Who-Is
        let no_response = timeout(
            Duration::from_secs(2),
            self.client.who_is(Some(self.device + 1), Some(self.device + 100)),
        ).await;
        // Should timeout (no I-Am from our device)
        match no_response {
            Ok(responses) => assert!(!responses.contains_device(self.device)),
            Err(_) => {} // timeout is also acceptable
        }
    }

    pub async fn run_all(&self) {
        self.test_device_object_required_properties().await;
        self.test_subscribecov_execute(ObjectId::analog_input(1)).await;
        self.test_error_response_unknown_property().await;
        self.test_who_is_i_am_conformance().await;
        // ... add all 135.1 test cases
    }
}
```

---

### 12.4 Load Tests

```rust
// tests/load/test_throughput.rs

use criterion::{criterion_group, criterion_main, Criterion, Throughput};

/// Sustained ReadProperty throughput
fn bench_read_property_throughput(c: &mut Criterion) {
    let rt = tokio::runtime::Runtime::new().unwrap();
    let (sim, client) = rt.block_on(setup_sim_with_devices(100, 100)).unwrap();

    let mut group = c.benchmark_group("read_property");
    group.throughput(Throughput::Elements(1));

    group.bench_function("sequential_100_devices", |b| {
        b.to_async(&rt).iter(|| async {
            for device_id in 1u32..=100 {
                client.read_property(
                    device_id,
                    ObjectId::analog_input(1),
                    PropertyIdentifier::PresentValue,
                    None,
                ).await.unwrap();
            }
        });
    });

    group.finish();
}

/// 10,000 device discovery load test
#[tokio::test]
#[ignore = "load test — run explicitly"]
async fn load_10k_device_who_is() {
    let sim = SimBuilder::new()
        .add_network(Network::ip("0.0.0.0:47808"))
        .add_devices(DeviceRange { ids: 1..=10_000, profile: "minimal" })
        .build().await.unwrap();

    let client = BacnetIpClient::connect("127.0.0.1:47808").await.unwrap();

    let start = Instant::now();
    let discovered = client.who_is(None, None).await.unwrap();
    let elapsed = start.elapsed();

    assert_eq!(discovered.len(), 10_000);
    println!("Discovered 10,000 devices in {elapsed:?}");
    assert!(elapsed < Duration::from_secs(30));
}

/// 1M objects — tick loop performance
#[tokio::test]
#[ignore = "load test"]
async fn load_1m_objects_tick_loop() {
    let sim = SimBuilder::new()
        .add_network(Network::ip("0.0.0.0:47808"))
        .add_devices(DeviceRange {
            ids: 1..=1000,
            profile: "large_ahu",   // 1000 objects per device = 1M total
        })
        .build().await.unwrap();

    // Run for 60 seconds, check tick does not fall behind
    tokio::time::sleep(Duration::from_secs(60)).await;

    let metrics = sim.get_metrics().await;
    assert!(
        metrics.tick_lag_seconds < 0.5,
        "Tick loop falling behind: {}s lag", metrics.tick_lag_seconds
    );
    assert_eq!(metrics.total_objects, 1_000_000);
}

/// COV notification throughput: 100K subscriptions
#[tokio::test]
#[ignore = "load test"]
async fn load_cov_100k_subscriptions() {
    let sim = SimBuilder::new()
        .add_network(Network::ip("0.0.0.0:47808"))
        .add_devices(DeviceRange { ids: 1..=1000, profile: "cov_heavy" })
        .build().await.unwrap();

    let mut clients: Vec<BacnetIpClient> = Vec::new();
    // 100 clients, each subscribing to 1000 objects
    for i in 0..100 {
        let client = BacnetIpClient::connect("127.0.0.1:47808").await.unwrap();
        for dev in 1u32..=1000 {
            client.subscribe_cov(CovSubscription {
                subscriber_process_id: i,
                monitored_object:      ObjectId::analog_input(1),
                device_id:             dev,
                issue_confirmed:       false,
                lifetime:              Some(3600),
                cov_increment:         None,
            }).await.unwrap();
        }
        clients.push(client);
    }

    // Total subscriptions = 100k
    let subs = sim.get_total_cov_subscriptions().await;
    assert_eq!(subs, 100_000);

    // Run for 10 seconds and count notifications
    tokio::time::sleep(Duration::from_secs(10)).await;

    let notif_sent = sim.get_metrics().await.cov_notifications_sent;
    println!("COV notifications sent in 10s: {notif_sent}");
    assert!(notif_sent > 0);
}

/// RPM throughput with concurrent clients
#[tokio::test]
#[ignore = "load test"]
async fn load_rpm_concurrent_clients() {
    let sim = SimBuilder::new()
        .add_network(Network::ip("0.0.0.0:47808"))
        .add_devices(DeviceRange { ids: 1..=100, profile: "standard_ahu" })
        .build().await.unwrap();

    let start = Instant::now();
    let requests = Arc::new(AtomicU64::new(0));
    let errors   = Arc::new(AtomicU64::new(0));

    let handles: Vec<_> = (0..50).map(|_| {
        let reqs = requests.clone();
        let errs = errors.clone();
        tokio::spawn(async move {
            let client = BacnetIpClient::connect("127.0.0.1:47808").await.unwrap();
            let deadline = Instant::now() + Duration::from_secs(30);
            while Instant::now() < deadline {
                let dev = rand::random::<u32>() % 100 + 1;
                match client.read_property_multiple(dev, all_properties_spec()).await {
                    Ok(_) => { reqs.fetch_add(1, Ordering::Relaxed); }
                    Err(_) => { errs.fetch_add(1, Ordering::Relaxed); }
                }
            }
        })
    }).collect();

    for h in handles { h.await.unwrap(); }

    let total = requests.load(Ordering::Relaxed);
    let failed = errors.load(Ordering::Relaxed);
    let rps = total as f64 / start.elapsed().as_secs_f64();
    println!("RPM RPS: {rps:.0}, errors: {failed}");
    assert!(rps > 1000.0, "Expected >1000 RPM/s, got {rps:.0}");
    assert_eq!(failed, 0, "No errors expected under load");
}
```

---

### 12.5 Chaos / Fault Injection Tests

```rust
// tests/chaos/test_fault_injection.rs

/// Packet loss: simulator remains consistent when 50% of packets are dropped
#[tokio::test]
async fn chaos_50_percent_packet_loss() {
    let sim = SimBuilder::new()
        .add_network(Network::ip_with_fault("127.0.0.1:47850", FaultConfig {
            packet_loss_pct: 50,
            ..Default::default()
        }))
        .add_devices(DeviceRange { ids: 1..=10, profile: "minimal" })
        .build().await.unwrap();

    let client = BacnetIpClient::builder("127.0.0.1:47850")
        .retries(5)
        .timeout(Duration::from_secs(3))
        .build().await.unwrap();

    // With retries, all reads should eventually succeed
    for device_id in 1u32..=10 {
        let result = client.read_property(
            device_id,
            ObjectId::analog_input(1),
            PropertyIdentifier::PresentValue,
            None,
        ).await;
        assert!(result.is_ok(), "Failed on device {device_id}: {result:?}");
    }
}

/// Slow client: COV notifications don't pile up or crash the simulator
#[tokio::test]
async fn chaos_slow_cov_consumer() {
    let sim = SimBuilder::new()
        .add_network(Network::ip("127.0.0.1:47851"))
        .add_device(DeviceConfig {
            id: 5000,
            objects: (1..=100)
                .map(|i| ObjectConfig::analog_input(i, &format!("AI-{i}"), Units::Kelvin)
                    .with_model(ValueModel::random_walk(20.0, 0.5, 0.0, 100.0)))
                .collect(),
        })
        .build().await.unwrap();

    let client = BacnetIpClient::connect("127.0.0.1:47851").await.unwrap();

    // Subscribe to all 100 objects
    for i in 1..=100 {
        client.subscribe_cov(CovSubscription {
            subscriber_process_id: i,
            monitored_object: ObjectId::analog_input(i),
            device_id: 5000,
            issue_confirmed: false,
            lifetime: Some(60),
            cov_increment: None,
        }).await.unwrap();
    }

    // Simulate slow consumer: read from channel with 100ms delay
    let mut rx = client.cov_notification_channel();
    tokio::spawn(async move {
        loop {
            tokio::time::sleep(Duration::from_millis(100)).await;
            let _ = rx.try_recv();
        }
    });

    // Run for 10 seconds — simulator memory should not grow unboundedly
    tokio::time::sleep(Duration::from_secs(10)).await;

    let metrics = sim.get_metrics().await;
    assert!(
        metrics.cov_notification_queue_depth < 10_000,
        "COV queue grew unboundedly: {}", metrics.cov_notification_queue_depth
    );
}

/// Device restart mid-operation: invoke ID table cleans up
#[tokio::test]
async fn chaos_device_restart_mid_request() {
    let sim = SimBuilder::new()
        .add_network(Network::ip("127.0.0.1:47852"))
        .add_device(DeviceConfig { id: 6000, objects: vec![
            ObjectConfig::analog_input(1, "AI-1", Units::Kelvin)
        ]})
        .build().await.unwrap();

    let client = BacnetIpClient::builder("127.0.0.1:47852")
        .timeout(Duration::from_millis(500))
        .build().await.unwrap();

    // Restart device during a pending request
    let device_handle = sim.get_device_handle(6000).await;
    let restart_task = tokio::spawn(async move {
        tokio::time::sleep(Duration::from_millis(100)).await;
        device_handle.restart().await;
    });

    let result = client.read_property(
        6000, ObjectId::analog_input(1), PropertyIdentifier::PresentValue, None
    ).await;

    restart_task.await.unwrap();

    // Either success (request completed before restart) or error (expected)
    // But the simulator itself must remain alive
    tokio::time::sleep(Duration::from_millis(200)).await;

    // After restart, device should respond again
    let post_restart = client.read_property(
        6000, ObjectId::analog_input(1), PropertyIdentifier::PresentValue, None
    ).await;
    assert!(post_restart.is_ok(), "Device did not recover: {post_restart:?}");
}

/// OOM simulation: adding beyond max capacity should error gracefully
#[tokio::test]
async fn chaos_reject_objects_beyond_capacity() {
    let sim = SimBuilder::new()
        .add_network(Network::ip("127.0.0.1:47853"))
        .max_objects(1000)
        .build().await.unwrap();

    let api = SimApiClient::connect("http://127.0.0.1:8080").await.unwrap();

    // Add 1000 objects — should succeed
    for i in 1..=1000 {
        api.create_analog_input(1, i).await.unwrap();
    }

    // Adding the 1001st should fail gracefully
    let overflow = api.create_analog_input(1, 1001).await;
    assert!(overflow.is_err());
    assert_eq!(overflow.unwrap_err().status(), 507); // 507 Insufficient Storage
}
```

---

### 12.6 Fuzz Tests

```rust
// fuzz/fuzz_targets/fuzz_bvll_decode.rs
#![no_main]
use libfuzzer_sys::fuzz_target;
use bacnet_codec::bvll::BvllFrame;

fuzz_target!(|data: &[u8]| {
    // Must not panic on any input
    let _ = BvllFrame::decode(data);
});

// fuzz/fuzz_targets/fuzz_apdu_decode.rs
fuzz_target!(|data: &[u8]| {
    let _ = bacnet_codec::apdu::ConfirmedRequest::decode(data);
    let _ = bacnet_codec::apdu::UnconfirmedRequest::decode(data);
    let _ = bacnet_codec::apdu::ComplexAck::decode(data);
});

// fuzz/fuzz_targets/fuzz_npdu_decode.rs
fuzz_target!(|data: &[u8]| {
    let _ = bacnet_codec::npdu::Npdu::decode(data);
});

// fuzz/fuzz_targets/fuzz_mstp_decode.rs
fuzz_target!(|data: &[u8]| {
    let _ = bacnet_codec::mstp::MstpFrame::decode(data);
});

// fuzz/fuzz_targets/fuzz_property_value_decode.rs
fuzz_target!(|data: &[u8]| {
    let _ = bacnet_types::PropertyValue::decode_application(data);
    let _ = bacnet_types::PropertyValue::decode_context(data, 0);
});
```

---

## 13. Implementation Phases & Milestones

### Phase 1 — Foundation (Weeks 1-4)

**Goal:** Working BACnet/IP endpoint responding to Who-Is/I-Am and ReadProperty

| Task | Crate | Deliverable |
|---|---|---|
| ASN.1 application tag encoder/decoder | `bacnet-types` | All primitive types encode/decode with tests |
| BVLL framing | `bacnet-codec` | All 12 BVLL function codes |
| NPDU encode/decode | `bacnet-codec` | Priority, hop count, routing fields |
| APDU confirmed/unconfirmed | `bacnet-codec` | All PDU types with roundtrip tests |
| UDP socket transport | `bacnet-transport` | RX/TX loops, broadcast |
| Who-Is / I-Am | `bacnet-stack` | Responds to global and range Who-Is |
| Device + AI object | `bacnet-object` | Required properties, ReadProperty |
| Basic simulator | `bacnet-sim` | Single device, constant value |

**Exit criteria:** Passes `Who-Is/I-Am` and `ReadProperty` integration tests. Verified with `BACnet Browser` tool against the simulator.

---

### Phase 2 — Object Model Completeness (Weeks 5-8)

| Task | Notes |
|---|---|
| All standard object types | AO, AV, BI, BO, BV, MSI, MSO, MSV, NC, Schedule |
| ReadPropertyMultiple | Including `ALL`, `REQUIRED`, `OPTIONAL` specifiers |
| WriteProperty + priority array | All 16 priorities, relinquish, out-of-service guard |
| WritePropertyMultiple | Atomic write semantics |
| Object list via ReadProperty array index | `Device.Object_List[N]` |
| Segmentation (responder side) | For large RPM responses |
| Error/Reject/Abort PDU generation | All error classes + codes |

**Exit criteria:** All integration tests in §12.2 green.

---

### Phase 3 — Simulation Engine (Weeks 9-12)

| Task | Notes |
|---|---|
| Value models | Sine, random walk, step, thermal, expression |
| COV engine | Subscribe, unsubscribe, notify, expiry |
| Alarm engine | Intrinsic reporting, state machines, ack |
| Trend Log object | TrendLog, TrendLogMultiple, ReadRange service |
| Schedule engine | BACnetSchedule, effective period, exception schedule |
| Simulation tick loop | Tokio + Rayon hybrid |

**Exit criteria:** COV and alarm integration tests green; 100K-object tick < 100ms.

---

### Phase 4 — Scale (Weeks 13-16)

| Task | Notes |
|---|---|
| Sharded object store | DashMap / custom N-shard RwLock |
| Bulk object insertion | Sorted-by-shard batch API |
| Device multiplexing | 10K devices on one socket |
| Memory profiling | Heaptrack / DHAT; meet 4GB/10M objects target |
| MS/TP virtual bus | In-process token ring |
| MS/TP serial bridge | tokio-serial integration |

**Exit criteria:** Load tests pass (10K devices, 1M objects).

---

### Phase 5 — BACnet/SC (Weeks 17-20)

| Task | Notes |
|---|---|
| SC frame codec | BVLC-SC functions, control flags, options |
| TLS acceptor/connector | rustls, mTLS, SCRAM-SHA-256 |
| SC Hub | WebSocket server, node registry, frame routing |
| SC Node | Client-mode WebSocket, reconnect logic |
| SC-IP router bridge | NPDU exchange across transport types |
| SC failover | Primary + failover hub connection |

**Exit criteria:** SC integration tests pass; cross-transport routing works.

---

### Phase 6 — Management & Observability (Weeks 21-22)

| Task | Notes |
|---|---|
| REST API (Axum) | CRUD for devices/objects, scenario control |
| Prometheus exporter | All metrics listed in §11 |
| gRPC API (Tonic) | Streaming subscription to value changes |
| Structured logging | `tracing` + JSON format |
| Config hot-reload | TOML config change without restart |
| OpenTelemetry traces | Per-request spans across services |

---

### Phase 7 — Hardening (Weeks 23-24)

| Task | Notes |
|---|---|
| Fuzz corpus: 1M inputs | cargo-fuzz, AFL++ |
| Chaos test suite | Packet loss, slow consumers, OOM |
| Conformance suite | ASHRAE 135.1 test procedures |
| CI pipeline | GitHub Actions: unit + integration + fuzz (30min) |
| Documentation | rustdoc + user guide |
| Benchmarks | Criterion suite, publish results |

---

## 14. Dependency Map

```toml
# Workspace Cargo.toml (key dependencies)

[workspace.dependencies]
# Async runtime
tokio       = { version = "1", features = ["full"] }
tokio-util  = { version = "0.7", features = ["codec"] }

# Parallelism
rayon       = "1.10"
crossbeam   = "0.8"

# Concurrent collections
dashmap     = "6"
parking_lot = "0.12"

# Byte handling
bytes       = "1"
bytesmut    = "1"     # via bytes

# Hashing
rustc-hash  = "2"    # FxHasher
ahash       = "0.8"

# Networking
tokio-serial  = "5"            # MS/TP serial
tokio-tungstenite = "0.23"     # SC WebSocket
rustls        = "0.23"         # SC TLS
rustls-pemfile = "2"
webpki        = "0.22"

# Crypto (SCRAM-SHA-256 for SC auth)
sha2       = "0.10"
hmac       = "0.12"
base64     = "0.22"

# Serialization
serde      = { version = "1", features = ["derive"] }
serde_json = "1"
toml       = "0.8"

# HTTP/gRPC
axum      = "0.7"
tonic     = "0.12"
prost     = "0.13"

# Metrics
prometheus = "0.13"
opentelemetry = "0.24"
tracing    = "0.1"
tracing-subscriber = "0.3"

# Scripting
rhai       = "1"

# Random
rand       = "0.8"
rand_small = "0.1"   # SmallRng for value models

# Testing
criterion  = { version = "0.5", features = ["async_tokio"] }

# Fuzzing (dev-dep)
libfuzzer-sys = "0.4"
```

---

## 15. Performance Targets

| Metric | Target | Measurement |
|---|---|---|
| Devices simulated | 10,000 | BACnet/IP: 8,000 + MS/TP: 1,500 + SC: 500 |
| Objects simulated | 10,000,000 | 1,000 objects/device avg |
| RSS memory | < 5 GB | For 10M objects |
| Tick latency p99 | < 100ms | For 1M-object tick at 1 Hz |
| Who-Is response time | < 5s | For 10K devices responding |
| ReadProperty throughput | > 50,000 req/s | 50 concurrent clients |
| RPM throughput | > 5,000 req/s | 50 concurrent clients, 100-property RPM |
| COV notifications | > 1M/s | Aggregate across all devices |
| COV subscription overhead | < 10µs/sub | Time to check a single subscription |
| BVLL encode/decode | > 10M frames/s | Single-core, criterion benchmark |
| MS/TP token period | < 1ms/station | For 127-station virtual bus |

---

*Document version: 1.0 — ASHRAE 135-2020 reference implementation plan*