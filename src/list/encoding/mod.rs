#[allow(unused)]
#[allow(unused_imports)]

/// The encoding module converts the internal data structures to and from a lossless compact binary
/// data format.
///
/// This is modelled after the run-length encoding in Automerge and Yjs.

// Notes for next time I break compatibility:
// - Version in encode::write_local_version - skip second 0 if its ROOT.

mod varint;
mod encode_oplog;
mod decode_oplog;

#[cfg(test)]
mod tests;
#[cfg(test)]
mod fuzzer;
pub mod encode_tools;
mod decode_tools;

use rle::MergableSpan;
use crate::list::encoding::varint::*;
use num_enum::TryFromPrimitive;
pub use encode_oplog::{ENCODE_FULL, ENCODE_PATCH, EncodeOptions};

const MAGIC_BYTES: [u8; 8] = *b"DMNDTYPS";

const PROTOCOL_VERSION: usize = 0;

// #[derive(Debug, PartialEq, Eq, Copy, Clone)]
#[derive(Debug, PartialEq, Eq, Copy, Clone, TryFromPrimitive)]
#[repr(u32)]
enum ChunkType {
    /// Packed bytes storing any data compressed in later parts of the file.
    CompressedFieldsLZ4 = 5,

    /// FileInfo contains optional UserData and AgentNames.
    FileInfo = 1,
    DocId = 2,
    AgentNames = 3,
    UserData = 4,

    /// The StartBranch chunk describes the state of the document before included patches have been
    /// applied.
    StartBranch = 10,
    Version = 12,
    /// StartBranch content is optional.
    Content = 13,
    ContentCompressed = 14, // Might make more sense to have a generic compression tag for chunks.

    Patches = 20,
    OpVersions = 21,
    OpTypeAndPosition = 22,
    OpParents = 23,

    PatchContent = 24,
    /// ContentKnown is a RLE expressing which ranges of patches have known content
    ContentIsKnown = 25,

    TransformedPositions = 27, // Currently unused

    Crc = 100,
}

#[derive(Debug, PartialEq, Eq, Copy, Clone, TryFromPrimitive)]
#[repr(u32)]
enum DataType {
    Bool = 1,
    VarUInt = 2,
    VarInt = 3,
    PlainText = 4,
}

#[derive(Debug, PartialEq, Eq, Copy, Clone, TryFromPrimitive)]
#[repr(u32)]
enum CompressionFormat {
    // Just for future proofing, ya know?
    LZ4 = 1,
}

fn checksum(data: &[u8]) -> u32 {
    // This is crc32c. Using the crc library because the resulting binary size is much smaller.
    // let checksum = crc32c::crc32c(&result);
    crc::Crc::<u32>::new(&crc::CRC_32_ISCSI).checksum(data)
}
