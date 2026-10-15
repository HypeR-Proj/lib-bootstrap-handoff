// SPDX-FileCopyrightText: 2026 roolrz
// SPDX-License-Identifier: Apache-2.0

//! Formal-side bootstrap inventory. Stage1 does not link this library. The
//! manager receives supervision once, commits the root, then moves domain
//! resources to its startup coordinator under a distinct committed protocol.

use hyper_component::objects::Object;
use hyper_os::{
    Error, Result,
    capability_channel::*,
    handle::*,
    memory::SnapshotVmo,
    startup::{Startup, StartupPurpose},
};
use serde::{Deserialize, Serialize};

pub const ENDPOINT: StartupPurpose<CapabilityChannelObject> = StartupPurpose::new(0x8008_0001);
pub const DESCRIPTION: StartupPurpose<VmoObject> = StartupPurpose::new(0x8008_0002);
pub const ROOT_TRANSITION: u16 = 37;
pub const COMMIT: &[u8] = b"HYPER-INIT-HANDOFF/3";
pub const FORWARD: &[u8] = b"HYPER-SYSTEM-STARTUP/1";

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct Slot {
    pub key: u16,
    pub rights: u64,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct CoreService {
    pub name: String,
    pub image: String,
    pub critical: bool,
    pub provider: Option<String>,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct Offer {
    pub version: u32,
    pub root_mount: u64,
    pub boot_root: String,
    pub services: Vec<CoreService>,
    pub storage_ready: bool,
    pub slots: Vec<Slot>,
}

impl Offer {
    pub fn read(handle: HandleRef<'_, VmoObject>) -> Result<Self> {
        let vmo = SnapshotVmo::from_vmo(handle)?;
        let mut length = [0; 8];
        vmo.read_exact_at(0, &mut length)?;
        let length =
            usize::try_from(u64::from_le_bytes(length)).map_err(|_| Error::InvalidResponse)?;
        if length > 32 * 1024 {
            return Err(Error::InvalidResponse);
        }
        let mut bytes = vec![0; length];
        vmo.read_exact_at(8, &mut bytes)?;
        let value: Self = serde_json::from_slice(&bytes).map_err(|_| Error::InvalidResponse)?;
        value.validate()?;
        Ok(value)
    }

    pub fn validate(&self) -> Result<()> {
        if self.version != 3
            || self.root_mount == 0
            || self.services.len() > 24
            || self.slots.len() > 64
            || !self.boot_root.starts_with('/')
            || self.boot_root == "/"
            || self.boot_root.len() > 255
            || self.boot_root.as_bytes().contains(&0)
        {
            return Err(Error::InvalidResponse);
        }
        let mut keys = std::collections::BTreeSet::new();
        for slot in &self.slots {
            kind(slot.key)?;
            if !keys.insert(slot.key)
                || Rights::from_bits(slot.rights).is_none()
                || (slot.key >= 100 && usize::from(slot.key - 100) >= self.services.len())
            {
                return Err(Error::InvalidResponse);
            }
        }
        for key in [1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 30, 31, 32, 33, 37] {
            if !keys.contains(&key) {
                return Err(Error::MissingHandle);
            }
        }
        let mut roles = std::collections::BTreeSet::new();
        for (index, service) in self.services.iter().enumerate() {
            if service.critical && !keys.contains(&(100 + index as u16)) {
                return Err(Error::MissingHandle);
            }
            if let Some(role) = &service.provider
                && (!matches!(role.as_str(), "console" | "io" | "filesystem")
                    || !roles.insert(role))
            {
                return Err(Error::InvalidResponse);
            }
        }
        Ok(())
    }

    pub fn slots(&self, forwarded: bool) -> Result<Vec<CapabilityReceiveSlot>> {
        self.slots
            .iter()
            .filter(|s| !forwarded || s.key != ROOT_TRANSITION)
            .map(|s| {
                Object::receive_slot(
                    kind(s.key)?,
                    Rights::from_bits(s.rights).ok_or(Error::InvalidResponse)?,
                )
            })
            .collect()
    }

    pub fn receive(
        &self,
        endpoint: &CapabilityChannel,
        forwarded: bool,
        until: u64,
    ) -> Result<Inventory> {
        let mut slots = self.slots(forwarded)?;
        let mut bytes = [core::mem::MaybeUninit::uninit(); 64];
        let message = endpoint.receive(until, &mut bytes, &mut slots)?;
        if message.bytes() != if forwarded { FORWARD } else { COMMIT }
            || message.capability_count() != slots.len()
        {
            return Err(Error::InvalidResponse);
        }
        let objects = self
            .slots
            .iter()
            .filter(|s| !forwarded || s.key != ROOT_TRANSITION)
            .zip(&mut slots)
            .map(|(s, received)| Ok((s.key, Object::take(kind(s.key)?, received)?)))
            .collect::<Result<_>>()?;
        Ok(Inventory { objects })
    }
}

pub struct Inventory {
    pub objects: Vec<(u16, Object)>,
}

impl Inventory {
    pub fn get(&self, key: u16) -> Result<&Object> {
        self.objects
            .iter()
            .find(|(k, _)| *k == key)
            .map(|(_, v)| v)
            .ok_or(Error::MissingHandle)
    }
    pub fn take(&mut self, key: u16) -> Result<Object> {
        let index = self
            .objects
            .iter()
            .position(|(k, _)| *k == key)
            .ok_or(Error::MissingHandle)?;
        Ok(self.objects.remove(index).1)
    }
    pub fn commit_root(&mut self) -> Result<()> {
        let Object::RootTransition(Some(token)) = self.get(ROOT_TRANSITION)? else {
            return Err(Error::InvalidResponse);
        };
        hyper_os::filesystem::finish_root_switch(token.as_handle_ref(), true)?;
        self.take(ROOT_TRANSITION)?;
        Ok(())
    }
    /// A rejected rendezvous retains every owner. The caller must retain this
    /// inventory on post-commit failure; it cannot claim bootstrap rolled back.
    pub fn forward(
        &mut self,
        offer: &Offer,
        channel: &CapabilityChannel,
        until: u64,
    ) -> Result<()> {
        let mut dispositions = Vec::new();
        for (key, object) in &mut self.objects {
            let slot = offer
                .slots
                .iter()
                .find(|s| s.key == *key)
                .ok_or(Error::InvalidResponse)?;
            dispositions.push(
                object.move_offer(Rights::from_bits(slot.rights).ok_or(Error::InvalidResponse)?)?,
            );
        }
        hyper_service_ipc::send(channel, until, FORWARD, &mut dispositions)
    }
}

pub fn kind(key: u16) -> Result<u32> {
    Ok(match key {
        1 | 2 | 32 | 33 => DirectoryObject::KIND.as_raw(),
        3 => TaskFactoryObject::KIND.as_raw(),
        4 | 30 => TaskGroupObject::KIND.as_raw(),
        5 => ResourceDomainObject::KIND.as_raw(),
        6 => TaskInspectorObject::KIND.as_raw(),
        7 => ObjectInspectorObject::KIND.as_raw(),
        8 => MemoryInspectorObject::KIND.as_raw(),
        9 => CpuInspectorObject::KIND.as_raw(),
        10 | 12..=14 | 16 | 17 | 19..=22 | 34 | 35 => CapabilityChannelObject::KIND.as_raw(),
        11 => DeviceAssignmentAuthorityObject::KIND.as_raw(),
        18 | 36 => ByteChannelObject::KIND.as_raw(),
        25 => VirtualMachineCreationAuthorityObject::KIND.as_raw(),
        31 => ConsoleObject::KIND.as_raw(),
        37 => RootTransitionObject::KIND.as_raw(),
        100..=123 => ProcessObject::KIND.as_raw(),
        _ => return Err(Error::InvalidResponse),
    })
}

pub fn lock_candidate(startup: &Startup<'_>, root_mount: u64) -> Result<()> {
    hyper_os::memory::lock_current_memory(startup.borrow(hyper_os::startup::ROOT_VMAR)?)?;
    let root = startup.borrow(hyper_os::startup::ROOT_DIRECTORY)?;
    let root = hyper_os::fs::Directory::from_handle(root.duplicate(root.info()?.rights)?);
    if root.info()?.location().mount().get() != root_mount {
        return Err(Error::InvalidResponse);
    }
    Ok(())
}

pub mod retirement;
