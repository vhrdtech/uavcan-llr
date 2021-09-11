#[deny(warnings)]

use core::fmt::{Formatter, Display};
use core::convert::{TryFrom};
use crate::Error;
use core::cmp::Ordering;
use hash32_derive::Hash32;

macro_rules! max_bound_number {
    ($type_name: ident, $base_type: ty, $max: literal) => {
        #[derive(Copy, Clone, Eq, PartialEq, Debug, Hash32)]
        pub struct $type_name($base_type);
        impl $type_name {
            pub const fn new(x: $base_type) -> Option<$type_name> {
                if x <= $max {
                    Some($type_name(x))
                } else {
                    None
                }
            }

            pub unsafe fn new_unchecked(x: $base_type) -> $type_name {
                $type_name(x)
            }

            pub const fn inner(&self) -> $base_type {
                self.0
            }
        }
    };
}

max_bound_number!(NodeId, u8, 127);
max_bound_number!(SubjectId, u16, 8191);
max_bound_number!(ServiceId, u16, 511);
max_bound_number!(TransferId, u8, 31);

impl TransferId {
    pub fn increment(&mut self) {
        if self.0 == 31 {
            self.0 = 0;
        } else {
            self.0 += 1;
        }
    }
}

impl Default for TransferId {
    fn default() -> Self {
        TransferId(0)
    }
}

#[derive(Copy, Clone, Eq, PartialEq, Debug)]
pub struct CanId {
    pub source_node_id: NodeId,
    pub transfer_kind: TransferKind,
    pub priority: Priority,
}
impl CanId {
    pub fn new_message_kind(source_node_id: NodeId, subject_id: SubjectId, is_anonymous: bool, priority: Priority) -> Self {
        CanId {
            source_node_id,
            transfer_kind: TransferKind::Message(Message {
                subject_id,
                is_anonymous
            }),
            priority
        }
    }

    pub fn new_service_kind(source_node_id: NodeId, destination_node_id: NodeId, service_id: ServiceId, is_request: bool, priority: Priority) -> Self {
        CanId {
            source_node_id,
            transfer_kind: TransferKind::Service(Service {
                destination_node_id,
                service_id,
                is_request
            }),
            priority
        }
    }
}
impl Display for CanId {
    fn fmt(&self, f: &mut Formatter<'_>) -> core::fmt::Result {
        write!(f, "N{:03} {}->", self.source_node_id.inner(), self.priority).ok();
        match self.transfer_kind {
            TransferKind::Message(message) => {
                let t = if message.is_anonymous { 'A' } else { '_' };
                write!(f, "M{}{:04}", t, message.subject_id.inner())
            }
            TransferKind::Service(service) => {
                let t = if service.is_request { "Rq" } else { "Rp" };
                write!(f, "N:{:03} S{}{:03}", service.destination_node_id.inner(), t, service.service_id.inner())
            }
        }
    }
}
impl TryFrom<u32> for CanId {
    type Error = Error;

    fn try_from(value: u32) -> Result<Self, Self::Error> {
        let high_bits = (value >> 29) & 7;
        if high_bits != 0 {
            return Err(Error::NoneZeroHighBits);
        }
        let reserved23 = value & (1 << 23) != 0;
        if reserved23 {
            return Err(Error::WrongReservedBit);
        }
        let source_node_id = value & 0b111_1111;
        let source_node_id = unsafe { NodeId::new_unchecked(source_node_id as u8) };
        let is_service = value & (1 << 25) != 0;
        let transfer_kind = if is_service {
            let destination_node_id = (value >> 7) & 127;
            let destination_node_id = unsafe { NodeId::new_unchecked(destination_node_id as u8) };
            let service_id = (value >> 14) & 511;
            let service_id = unsafe { ServiceId::new_unchecked(service_id as u16) };
            let is_request = value & (1 << 24) != 0;
            TransferKind::Service(Service{
                destination_node_id,
                service_id,
                is_request
            })
        } else { // is_message
            let reserved7 = value & (1 << 7) != 0;
            if reserved7 {
                return Err(Error::WrongReservedBit);
            }
            let subject_id = (value >> 8) & 8191;
            let subject_id = unsafe { SubjectId::new_unchecked(subject_id as u16) };
            let is_anonymous = value & (1 << 24) != 0;
            TransferKind::Message(Message {
                subject_id,
                is_anonymous
            })
        };
        let priority = (value >> 26) & 7;
        let priority = Priority::new(priority as u8).unwrap();
        Ok(CanId {
            source_node_id,
            transfer_kind,
            priority,
        })
    }
}
impl Into<u32> for CanId {
    fn into(self) -> u32 {
        let source_id = self.source_node_id.inner() as u32;
        let priority = (self.priority as u32) << 26;
        let bits26_7 = self.transfer_kind.ser();
        priority | bits26_7 | source_id
    }
}
#[cfg(feature = "vhrdcan")]
impl Into<vhrdcan::FrameId> for CanId {
    fn into(self) -> vhrdcan::FrameId {
        unsafe { vhrdcan::FrameId::Extended(vhrdcan::id::ExtendedId::new_unchecked(self.into())) }
    }
}
#[cfg(feature = "vhrdcan")]
impl TryFrom<vhrdcan::FrameId> for CanId {
    type Error = Error;

    fn try_from(frame_id: vhrdcan::FrameId) -> Result<Self, Self::Error> {
        use vhrdcan::FrameId;
        match frame_id {
            FrameId::Standard(_) => Err(Error::StandardIdNotSupported),
            FrameId::Extended(eid) => CanId::try_from(eid.inner())
        }
    }
}
// impl Ord for CanId {
//     fn cmp(&self, other: &Self) -> Ordering {
//         self.priority.cmp(&other.priority)
//     }
// }
// impl PartialOrd for CanId {
//     fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
//         Some(self.cmp(other))
//     }
// }

#[derive(Copy, Clone, Eq, PartialEq, Debug)]
pub enum TransferKind {
    Message(Message),
    Service(Service)
}
impl TransferKind {
    // Used in assembler.rs alongside source id to form a key into key-transfer map.
    pub(crate) fn ser(&self) -> u32 {
        match self {
            TransferKind::Message(message) => {
                let subject_id = (message.subject_id.inner() as u32) << 8;
                let is_anonymous = (message.is_anonymous as u32) << 24;
                subject_id | is_anonymous
            }
            TransferKind::Service(service) => {
                let destination_id = (service.destination_node_id.inner() as u32) << 7;
                let service_id = (service.service_id.inner() as u32) << 14;
                let is_request = (service.is_request as u32) << 24;
                let is_service = 1 << 25;
                destination_id | service_id | is_request | is_service
            }
        }
    }
}
// Hash32 doesn't support enums as of writing this, so u32 is used instead.
impl hash32::Hash for TransferKind {
    fn hash<H: hash32::Hasher>(&self, state: &mut H) {
        let bits26_7 = self.ser().to_le_bytes();
        state.write(&bits26_7);
    }
}

#[derive(Copy, Clone, Eq, PartialEq, Debug)]
pub struct Message {
    pub subject_id: SubjectId,
    pub is_anonymous: bool,
}

#[derive(Copy, Clone, Eq, PartialEq, Debug)]
pub struct Service {
    pub destination_node_id: NodeId,
    pub service_id: ServiceId,
    pub is_request: bool,
}

#[derive(Copy, Clone, Eq, PartialEq, Debug)]
pub enum Priority {
    Exceptional = 0,
    Immediate = 1,
    Fast = 2,
    High = 3,
    Nominal = 4,
    Low = 5,
    Slow = 6,
    Optional = 7,
}
impl Priority {
    pub fn new(priority: u8) -> Option<Priority> {
        use Priority::*;
        match priority {
            0 => Some(Exceptional),
            1 => Some(Immediate),
            2 => Some(Fast),
            3 => Some(High),
            4 => Some(Nominal),
            5 => Some(Low),
            6 => Some(Slow),
            7 => Some(Optional),
            _ => None
        }
    }
}
impl Display for Priority {
    fn fmt(&self, f: &mut Formatter<'_>) -> core::fmt::Result {
        let c = r#"EIFHNLSO"#;
        write!(f, "{}", c.chars().nth(*self as usize).unwrap())
    }
}
impl PartialOrd for Priority {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}
impl Ord for Priority {
    fn cmp(&self, other: &Self) -> Ordering {
        (*self as u8).cmp(&(*other as u8)).reverse()
    }
}