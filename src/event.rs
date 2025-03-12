// SPDX-FileCopyrightText: © 2025 Claudio Cicconetti <c.cicconetti@iit.cnr.it>
// SPDX-License-Identifier: MIT

use crate::output::Sample;

#[derive(Debug, PartialEq, Eq)]
pub struct EprGeneratedData {
    pub tx_node_id: u32,
    pub master_node_id: u32,
    pub slave_node_id: u32,
}

#[derive(Debug, PartialEq, Eq)]
pub struct EprNotifiedData {
    pub this_node_id: u32,
    pub peer_node_id: u32,
    pub role: crate::nic::Role,
    pub epr_pair_id: u64,
}

#[derive(Debug, PartialEq, Eq)]
pub enum EventType {
    /// The warm-up period expires.
    WarmupPeriodEnd,
    /// The simulation ends.
    ExperimentEnd,
    /// Print progress.
    Progress(u16),

    /// New EPR generated by a tx.
    EprGenerated(EprGeneratedData),
    /// EPR pair notified at a node.
    EprNotified(EprNotifiedData),
}

/// A simulation event.
#[derive(PartialEq, Eq)]
pub struct Event {
    time: u64,
    pub event_type: EventType,
}

impl Event {
    pub fn new(time: f64, event_type: EventType) -> Self {
        Self {
            time: crate::utils::to_nanoseconds(time),
            event_type,
        }
    }

    pub fn time(&self) -> u64 {
        self.time
    }

    pub fn advance(&mut self, advance_time: u64) {
        self.time += advance_time
    }
}

#[allow(clippy::non_canonical_partial_ord_impl)]
impl PartialOrd for Event {
    fn partial_cmp(&self, other: &Self) -> Option<std::cmp::Ordering> {
        other.time().partial_cmp(&self.time())
    }
}

impl Ord for Event {
    fn cmp(&self, other: &Self) -> std::cmp::Ordering {
        self.partial_cmp(other).unwrap()
    }
}

pub trait EventHandler {
    fn handle(&mut self, event: Event) -> (Vec<Event>, Vec<Sample>);
    fn initial(&mut self) -> Vec<Event>;
}
