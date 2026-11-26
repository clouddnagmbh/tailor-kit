// SPDX-License-Identifier: Apache-2.0
// Copyright 2026 The RequestTailor Authors

//! A version-2 pack of whole objects (no deltas) for push, written with gix-pack.

use gix::ObjectId;
use gix_pack::data::output;

use crate::util::{Context, Result, fail};

/// The pack holding `ids` (any order), with its trailing checksum.
pub fn write(repo: &gix::Repository, ids: &[ObjectId]) -> Result<Vec<u8>> {
    let hash = repo.object_hash();
    let mut entries = Vec::with_capacity(ids.len());
    for id in ids {
        let object = repo.find_object(*id).ctx("cannot read an object")?;
        let data = gix::objs::Data {
            kind: object.kind,
            object_hash: hash,
            data: &object.data,
        };
        let count = output::Count::from_data(*id, None);
        entries.push(
            output::Entry::from_data(&count, &data, gix::zlib::Compression::DEFAULT)
                .ctx("cannot compress an object")?,
        );
    }
    let count = u32::try_from(entries.len()).map_err(|_| fail("too many objects to push"))?;
    let mut out = Vec::new();
    let writer = output::bytes::FromEntriesIter::new(
        std::iter::once(Ok::<_, gix::Error>(entries)),
        &mut out,
        count,
        gix_pack::data::Version::V2,
        hash,
    );
    for written in writer {
        written.ctx("cannot write the pack")?;
    }
    Ok(out)
}
