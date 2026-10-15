// SPDX-FileCopyrightText: 2026 roolrz
// SPDX-License-Identifier: Apache-2.0

//! Private manager-to-retirement oneshot resource transfer, not a public API.
use hyper_os::{
    handle::{CapabilityChannelObject, Rights},
    startup::StartupPurpose,
};
use serde::{Deserialize, Serialize};

pub const MAX_PREDECESSORS: usize = 48;
pub const ENDPOINT: StartupPurpose<CapabilityChannelObject> = StartupPurpose::new(0x800b_0020);
pub const ROOT: Rights = Rights::WRITE
    .union(Rights::EXECUTE)
    .union(Rights::READ)
    .union(Rights::INSPECT)
    .union(Rights::MANAGE_NAMESPACE);
pub const GROUP: Rights = Rights::WAIT
    .union(Rights::INSPECT)
    .union(Rights::REQUEST_STOP);
pub const PROCESS: Rights = Rights::WAIT
    .union(Rights::INSPECT)
    .union(Rights::REQUEST_STOP);
pub const DRAIN: Rights = Rights::READ.union(Rights::WRITE).union(Rights::WAIT);
#[derive(Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct Offer {
    pub path: String,
    pub processes: usize,
    pub console: Option<usize>,
    pub drain: bool,
}
