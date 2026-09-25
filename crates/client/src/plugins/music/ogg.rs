//! What the client reads of an Ogg Vorbis file before it plays it: the
//! comments its generator tagged it with, and its length. Both assume one
//! logical stream, which is all a generated file carries.

use std::time::Duration;

struct Page<'a> {
    granule: u64,
    lacing: &'a [u8],
    body: &'a [u8],
}

fn pages(bytes: &[u8]) -> impl Iterator<Item = Page<'_>> {
    let mut at = 0;
    std::iter::from_fn(move || {
        let head = bytes.get(at..at + 27)?;
        if &head[..4] != b"OggS" {
            return None;
        }
        let lacing = bytes.get(at + 27..at + 27 + head[26] as usize)?;
        let start = at + 27 + lacing.len();
        let size: usize = lacing.iter().map(|&l| l as usize).sum();
        let body = bytes.get(start..start + size)?;
        at = start + size;
        Some(Page { granule: u64::from_le_bytes(head[6..14].try_into().unwrap()), lacing, body })
    })
}

/// The stream's first `count` packets, joined across pages: a lacing value
/// under 255 closes a packet, 255 carries it on, into the next page if need be.
fn packets(bytes: &[u8], count: usize) -> Vec<Vec<u8>> {
    let mut out = Vec::new();
    let mut packet = Vec::new();
    for page in pages(bytes) {
        let mut at = 0;
        for &l in page.lacing {
            packet.extend_from_slice(&page.body[at..at + l as usize]);
            at += l as usize;
            if l < 255 {
                out.push(std::mem::take(&mut packet));
                if out.len() == count {
                    return out;
                }
            }
        }
    }
    out
}

fn u32_at(bytes: &[u8], at: usize) -> Option<u32> {
    Some(u32::from_le_bytes(bytes.get(at..at + 4)?.try_into().ok()?))
}

/// The Vorbis comments as `(KEY, value)`, the key upper-cased — Vorbis
/// keys are case-blind. `None` when the second packet is not a comment
/// header.
pub fn comments(bytes: &[u8]) -> Option<Vec<(String, String)>> {
    let header = packets(bytes, 2).pop()?;
    if header.get(..7)? != b"\x03vorbis" {
        return None;
    }
    let mut at = 7;
    at += 4 + u32_at(&header, at)? as usize;
    let count = u32_at(&header, at)?;
    at += 4;
    let mut out = Vec::new();
    for _ in 0..count {
        let len = u32_at(&header, at)? as usize;
        let text = std::str::from_utf8(header.get(at + 4..at + 4 + len)?).ok()?;
        at += 4 + len;
        if let Some((key, value)) = text.split_once('=') {
            out.push((key.to_ascii_uppercase(), value.to_string()));
        }
    }
    Some(out)
}

/// How long the stream plays: the last page's granule position is its
/// sample count, over the identification header's rate.
pub fn length(bytes: &[u8]) -> Option<Duration> {
    let ident = packets(bytes, 1).pop()?;
    if ident.get(..7)? != b"\x01vorbis" {
        return None;
    }
    let rate = u32_at(&ident, 12)?;
    // A page no packet ends on carries a granule of all ones.
    let samples = pages(bytes).map(|p| p.granule).filter(|&g| g != u64::MAX).last()?;
    (rate > 0).then(|| Duration::from_secs_f64(samples as f64 / rate as f64))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn laced(len: usize) -> Vec<u8> {
        let mut lacing = vec![255; len / 255];
        lacing.push((len % 255) as u8);
        lacing
    }

    fn page(granule: u64, lacing: &[u8], body: &[u8]) -> Vec<u8> {
        let mut out = b"OggS".to_vec();
        out.extend([0, 0]);
        out.extend(granule.to_le_bytes());
        out.extend([0; 12]);
        out.push(lacing.len() as u8);
        out.extend(lacing);
        out.extend(body);
        out
    }

    fn ident(rate: u32) -> Vec<u8> {
        let mut out = b"\x01vorbis".to_vec();
        out.extend([0, 0, 0, 0, 2]);
        out.extend(rate.to_le_bytes());
        out.extend([0; 14]);
        out
    }

    fn comment_header(vendor: &str, tags: &[&str]) -> Vec<u8> {
        let mut out = b"\x03vorbis".to_vec();
        out.extend((vendor.len() as u32).to_le_bytes());
        out.extend(vendor.bytes());
        out.extend((tags.len() as u32).to_le_bytes());
        for tag in tags {
            out.extend((tag.len() as u32).to_le_bytes());
            out.extend(tag.bytes());
        }
        out
    }

    #[test]
    fn comments_join_a_packet_split_across_pages() {
        let first = ident(48_000);
        let vendor = "v".repeat(600);
        let second = comment_header(&vendor, &["pool=overworld", "LOOP=true", "TITLE=a=b"]);
        let lacing = laced(second.len());
        // Split after two full segments, so the packet runs on into page two.
        let (head, tail) = lacing.split_at(2);
        let mut bytes = page(0, &laced(first.len()), &first);
        bytes.extend(page(0, head, &second[..510]));
        bytes.extend(page(0, tail, &second[510..]));

        let tags = comments(&bytes).unwrap();
        assert_eq!(tags, vec![
            ("POOL".to_string(), "overworld".to_string()),
            ("LOOP".to_string(), "true".to_string()),
            ("TITLE".to_string(), "a=b".to_string()),
        ]);
    }

    #[test]
    fn length_reads_the_last_ended_page() {
        let first = ident(48_000);
        let mut bytes = page(0, &laced(first.len()), &first);
        bytes.extend(page(48_000 * 3, &[4], &[0; 4]));
        bytes.extend(page(u64::MAX, &[255], &[0; 255]));
        assert_eq!(length(&bytes), Some(Duration::from_secs(3)));
    }

    #[test]
    fn a_file_that_is_not_vorbis_reads_as_nothing() {
        assert_eq!(comments(b"RIFF...."), None);
        assert_eq!(length(&page(10, &[3], b"abc")), None);
    }
}
