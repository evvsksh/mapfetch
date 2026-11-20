use anyhow::{Context, Result, bail};
use std::convert::TryFrom;

use super::{MemoryReader, regions::MemoryRegion};

const CHUNK_SIZE: usize = 8 * 1024 * 1024;
const MAX_REGION_SIZE: usize = 64 * 1024 * 1024;
const ADDRESS_SPACE_END: u64 = u32::MAX as u64 + 1;

const MEM_COMMIT: u32 = 0x1000;
const SCANNABLE_PROTECTIONS: u32 = 0x04 | 0x40;

#[derive(Debug, Clone)]
pub struct Pattern {
    bytes: Vec<Option<u8>>,
    offset: i32,
    non_zero_mask: bool,
    anchor: usize,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PatternMatch {
    pub index: usize,
    pub match_address: u32,
    pub address: u32,
}

impl Pattern {
    pub fn parse(value: &str) -> Result<Self> {
        Self::parse_with_options(value, 0, false)
    }

    pub fn parse_with_options(value: &str, offset: i32, non_zero_mask: bool) -> Result<Self> {
        let bytes = value
            .split_whitespace()
            .map(|token| match token {
                "??" => Ok(None),
                _ if token.len() == 2 => Ok(Some(u8::from_str_radix(token, 16)?)),
                _ => bail!("invalid pattern byte: {token}"),
            })
            .collect::<Result<Vec<_>>>()?;

        let anchor = bytes
            .iter()
            .position(Option::is_some)
            .context("pattern must contain at least one fixed byte")?;

        Ok(Self {
            bytes,
            offset,
            non_zero_mask,
            anchor,
        })
    }

    pub fn len(&self) -> usize {
        self.bytes.len()
    }

    fn find(&self, data: &[u8], start: usize) -> Option<usize> {
        let len = self.len();

        if len == 0 || start > data.len().saturating_sub(len) {
            return None;
        }

        let expected = self.bytes[self.anchor].unwrap();

        data[start..]
            .windows(len)
            .position(|window| {
                window[self.anchor] == expected
                    && self.bytes.iter().zip(window).all(|(expected, actual)| {
                        expected.is_none_or(|byte| byte == *actual)
                            && (!self.non_zero_mask || expected.is_some() || *actual != 0)
                    })
            })
            .map(|position| position + start)
    }

    fn resolve_address(&self, address: u32) -> Result<u32> {
        u32::try_from(i64::from(address) + i64::from(self.offset)).with_context(|| {
            format!(
                "pattern offset {:#X} moves address {address:#010X} outside the 32-bit address space",
                self.offset
            )
        })
    }
}

pub fn scan_first(
    reader: &MemoryReader,
    regions: &[MemoryRegion],
    pattern: &Pattern,
) -> Result<Option<PatternMatch>> {
    Ok(scan_many(reader, regions, std::slice::from_ref(pattern))?.pop())
}

pub fn scan_many(
    reader: &MemoryReader,
    regions: &[MemoryRegion],
    patterns: &[Pattern],
) -> Result<Vec<PatternMatch>> {
    Ok(scan_internal(reader, regions, patterns, true)?
        .into_iter()
        .filter_map(|matches| matches.into_iter().next())
        .collect())
}

fn scan_internal(
    reader: &MemoryReader,
    regions: &[MemoryRegion],
    patterns: &[Pattern],
    first_only: bool,
) -> Result<Vec<Vec<PatternMatch>>> {
    if patterns.is_empty() {
        return Ok(Vec::new());
    }

    let tail_capacity = patterns.iter().map(Pattern::len).max().unwrap_or(0) - 1;
    let mut chunk_buffer = vec![0; CHUNK_SIZE + tail_capacity];
    let mut region_buffer = Vec::new();
    let mut results = vec![Vec::new(); patterns.len()];

    for region in regions {
        if first_only && results.iter().all(|matches| !matches.is_empty()) {
            break;
        }

        if !is_scannable(region) {
            continue;
        }

        let Some((start, end)) = region_bounds(region)? else {
            continue;
        };

        let size = usize::try_from(end - start)?;
        let address = u32::try_from(start)?;

        if size <= MAX_REGION_SIZE && region_buffer.try_reserve_exact(size).is_ok() {
            region_buffer.resize(size, 0);

            if reader.read_exact(address, &mut region_buffer).is_ok() {
                scan_buffer(&region_buffer, start, 0, patterns, first_only, &mut results)?;
                continue;
            }

            region_buffer.clear();
        }

        scan_chunked(
            reader,
            start,
            end,
            patterns,
            first_only,
            tail_capacity,
            &mut results,
            &mut chunk_buffer,
        )?;
    }

    Ok(results)
}

fn scan_buffer(
    data: &[u8],
    base: u64,
    overlap: usize,
    patterns: &[Pattern],
    first_only: bool,
    results: &mut [Vec<PatternMatch>],
) -> Result<()> {
    for (index, pattern) in patterns.iter().enumerate() {
        if first_only && !results[index].is_empty() {
            continue;
        }

        let mut offset = overlap.saturating_sub(pattern.len() - 1);

        while let Some(position) = pattern.find(data, offset) {
            let match_address = u32::try_from(base + position as u64)?;

            results[index].push(PatternMatch {
                index,
                match_address,
                address: pattern.resolve_address(match_address)?,
            });

            if first_only {
                break;
            }

            offset = position + 1;
        }
    }

    Ok(())
}

fn scan_chunked(
    reader: &MemoryReader,
    start: u64,
    end: u64,
    patterns: &[Pattern],
    first_only: bool,
    tail_capacity: usize,
    results: &mut [Vec<PatternMatch>],
    buffer: &mut [u8],
) -> Result<()> {
    let (mut cursor, mut tail) = (start, 0);

    while cursor < end {
        if first_only && results.iter().all(|matches| !matches.is_empty()) {
            break;
        }

        let size = (end - cursor).min(CHUNK_SIZE as u64) as usize;
        let address = u32::try_from(cursor)?;

        if reader
            .read_exact(address, &mut buffer[tail..tail + size])
            .is_err()
        {
            tail = 0;
            cursor += size as u64;
            continue;
        }

        let data_len = tail + size;

        scan_buffer(
            &buffer[..data_len],
            cursor - tail as u64,
            tail,
            patterns,
            first_only,
            results,
        )?;

        tail = tail_capacity.min(data_len);

        if tail > 0 {
            buffer.copy_within(data_len - tail..data_len, 0);
        }

        cursor += size as u64;
    }

    Ok(())
}

fn is_scannable(region: &MemoryRegion) -> bool {
    region.is_readable()
        && region.state & MEM_COMMIT != 0
        && region.protection & SCANNABLE_PROTECTIONS != 0
}

fn region_bounds(region: &MemoryRegion) -> Result<Option<(u64, u64)>> {
    let start = u64::try_from(region.base_address)?;
    let end = start
        .saturating_add(u64::try_from(region.size)?)
        .min(ADDRESS_SPACE_END);

    Ok((start < end).then_some((start, end)))
}
