//! Stand-in transport for systems the launcher can't drive Wii Remotes on yet.
//! The types are uninhabited: `open` always fails, so nothing else is reachable.

use super::Availability;

pub enum Bluetooth {}
pub enum Discovery {}
pub enum Link {}

pub struct Seen {
    pub addr: String,
    pub name: String,
    pub in_range: bool,
    pub connected: bool,
}

impl Link {
    pub async fn send(&self, _report: &[u8]) -> Result<(), String> {
        match *self {}
    }

    pub async fn recv(&self, _buf: &mut [u8]) -> Result<usize, String> {
        match *self {}
    }
}

impl Bluetooth {
    pub async fn open() -> Result<Self, Availability> {
        Err(Availability::Unsupported)
    }

    pub async fn powered(&self) -> bool {
        match *self {}
    }

    pub async fn discover(&self) -> Result<Discovery, String> {
        match *self {}
    }

    pub async fn remotes(&self) -> Vec<Seen> {
        match *self {}
    }

    pub async fn connect(&self, _addr: &str) -> Result<Link, String> {
        match *self {}
    }

    pub async fn release(&self, _addr: &str) -> Result<(), String> {
        match *self {}
    }
}
