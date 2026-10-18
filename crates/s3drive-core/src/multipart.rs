//! Multipart geometry only. No payload, persistence, provider capability,
//! credential binding or permission to initiate/send/complete an upload.
use std::fmt;

/// Client resource ceiling, not an assertion about any provider's capabilities.
pub const MAX_PARTS: u16 = 10_000;
/// SQLite staging represents lengths using signed 64-bit integers.
pub const MAX_OBJECT_BYTES: u64 = i64::MAX as u64;

/// Explicit caller-supplied limits. There are no implicit provider defaults.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Limits {
    pub max_object_bytes: u64,
    pub min_nonfinal_part_bytes: u64,
    pub max_part_bytes: u64,
    pub max_parts: u16,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Error {
    InvalidLimits,
    InvalidPreferredSize,
    EmptyObject,
    ObjectTooLarge,
    CannotFit,
}
impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            Self::InvalidLimits => "invalid multipart sizing limits",
            Self::InvalidPreferredSize => "invalid preferred multipart size",
            Self::EmptyObject => "empty objects require the single-PUT path",
            Self::ObjectTooLarge => "object exceeds the configured multipart size limit",
            Self::CannotFit => "object cannot fit within the multipart part limits",
        })
    }
}
impl std::error::Error for Error {}

/// Immutable sizing result; this does not freeze bytes or grant a send permit.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Plan {
    total_bytes: u64,
    part_bytes: u64,
    part_count: u16,
    limits: Limits,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Part {
    pub number: u16,
    pub offset: u64,
    pub length: u64,
}

impl Plan {
    /// Raise the preference to the nonfinal minimum or count-required size.
    /// Reject impossible inputs; never relax limits or change the object size.
    pub fn new(total_bytes: u64, preferred_bytes: u64, limits: Limits) -> Result<Self, Error> {
        if limits.max_object_bytes == 0
            || limits.max_object_bytes > MAX_OBJECT_BYTES
            || limits.min_nonfinal_part_bytes == 0
            || limits.max_part_bytes < limits.min_nonfinal_part_bytes
            || limits.max_part_bytes > MAX_OBJECT_BYTES
            || !(1..=MAX_PARTS).contains(&limits.max_parts)
        {
            return Err(Error::InvalidLimits);
        }
        if preferred_bytes == 0 || preferred_bytes > limits.max_part_bytes {
            return Err(Error::InvalidPreferredSize);
        }
        if total_bytes == 0 {
            return Err(Error::EmptyObject);
        }
        if total_bytes > limits.max_object_bytes {
            return Err(Error::ObjectTooLarge);
        }
        // div_ceil avoids the overflowing (size + count - 1) formulation.
        let part_bytes = preferred_bytes
            .max(limits.min_nonfinal_part_bytes)
            .max(total_bytes.div_ceil(u64::from(limits.max_parts)));
        if part_bytes > limits.max_part_bytes {
            return Err(Error::CannotFit);
        }
        let part_count =
            u16::try_from(total_bytes.div_ceil(part_bytes)).map_err(|_| Error::CannotFit)?;
        Ok(Self {
            total_bytes,
            part_bytes,
            part_count,
            limits,
        })
    }

    pub fn total_bytes(&self) -> u64 {
        self.total_bytes
    }
    /// Stride for nonfinal parts; the last part may be shorter.
    pub fn part_bytes(&self) -> u64 {
        self.part_bytes
    }
    pub fn part_count(&self) -> u16 {
        self.part_count
    }
    pub fn limits(&self) -> Limits {
        self.limits
    }
    /// Part numbers are one-based. Invalid numbers have no corresponding bytes.
    pub fn part(&self, number: u16) -> Option<Part> {
        if !(1..=self.part_count).contains(&number) {
            return None;
        }
        Some(self.numbered_part(number))
    }
    /// O(1) iterator state; does not allocate a part vector or read payloads.
    pub fn parts(&self) -> impl ExactSizeIterator<Item = Part> + '_ {
        (1..=self.part_count).map(|number| self.numbered_part(number))
    }
    fn numbered_part(&self, number: u16) -> Part {
        // Valid numbers are <= ceil(total / stride), so this product is
        // strictly below total <= i64::MAX; subtraction cannot underflow.
        let offset = u64::from(number - 1) * self.part_bytes;
        Part {
            number,
            offset,
            length: self.part_bytes.min(self.total_bytes - offset),
        }
    }
}

#[cfg(test)]
mod tests;
