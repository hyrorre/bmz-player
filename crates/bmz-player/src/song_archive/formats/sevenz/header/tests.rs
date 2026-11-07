use super::*;
use std::io::Cursor;

const ENCODED: &[u8] = include_bytes!("../../../fixtures/7zip-encoded-header.7z");
const PLAIN: &[u8] = include_bytes!("../../../fixtures/7zip-plain-header.7z");

fn header(bytes: &[u8]) -> &[u8] {
    let offset = 32 + u64::from_le_bytes(bytes[12..20].try_into().unwrap()) as usize;
    &bytes[offset..]
}

fn number(value: u64) -> Vec<u8> {
    let mut bytes = vec![0xff];
    bytes.extend(value.to_le_bytes());
    bytes
}

fn replace_header(original: &[u8], header: &[u8]) -> Vec<u8> {
    let offset = 32 + u64::from_le_bytes(original[12..20].try_into().unwrap()) as usize;
    let mut bytes = original[..offset].to_vec();
    bytes[20..28].copy_from_slice(&(header.len() as u64).to_le_bytes());
    bytes[28..32].copy_from_slice(&checksum(header).to_le_bytes());
    let crc = checksum(&bytes[12..32]);
    bytes[8..12].copy_from_slice(&crc.to_le_bytes());
    bytes.extend(header);
    bytes
}

// The real 7-Zip fixture uses one LZMA header coder. Locate its properties and
// unpack-size field without involving a decompressor.
fn encoded_fields() -> (std::ops::Range<usize>, std::ops::Range<usize>) {
    let bytes = header(ENCODED);
    let mut input = Input(bytes);
    input.expect(0x17).unwrap();
    input.expect(6).unwrap();
    input.number().unwrap();
    assert_eq!(input.count().unwrap(), 1);
    input.expect(9).unwrap();
    input.number().unwrap();
    let mut id = input.byte().unwrap();
    if id == 10 {
        input.digests(1).unwrap();
        id = input.byte().unwrap();
    }
    assert_eq!(id, 0);
    input.expect(7).unwrap();
    input.expect(11).unwrap();
    assert_eq!(input.count().unwrap(), 1);
    input.expect(0).unwrap();
    assert_eq!(input.count().unwrap(), 1);
    let flags = input.byte().unwrap();
    assert_eq!(input.take((flags & 15) as usize).unwrap(), [3, 1, 1]);
    assert_eq!(flags & 0x30, 0x20);
    assert_eq!(input.count().unwrap(), 5);
    let properties = bytes.len() - input.0.len();
    input.take(5).unwrap();
    input.expect(12).unwrap();
    let size = bytes.len() - input.0.len();
    input.number().unwrap();
    (properties + 1..properties + 5, size..bytes.len() - input.0.len())
}

struct HeaderOnly {
    cursor: Cursor<Vec<u8>>,
    position: u64,
}

impl Read for HeaderOnly {
    fn read(&mut self, bytes: &mut [u8]) -> std::io::Result<usize> {
        let position = self.cursor.position();
        assert!(
            position < 32 || position >= self.position,
            "packed data was read before validation"
        );
        self.cursor.read(bytes)
    }
}

impl Seek for HeaderOnly {
    fn seek(&mut self, position: SeekFrom) -> std::io::Result<u64> {
        self.cursor.seek(position)
    }
}

fn preflight_without_packed_reads(bytes: Vec<u8>, limits: &ArchiveLimits) -> Result<()> {
    let position = 32 + u64::from_le_bytes(bytes[12..20].try_into().unwrap());
    preflight(&mut HeaderOnly { cursor: Cursor::new(bytes), position }, limits)
}

#[test]
fn seven_zip_compressed_and_plain_headers_read_scan_and_extract() {
    for (bytes, id) in [(ENCODED, 0x17), (PLAIN, 1)] {
        assert_eq!(header(bytes)[0], id);
        preflight_without_packed_reads(bytes.to_vec(), &Default::default()).unwrap();
        let root = tempfile::tempdir().unwrap();
        let path = root.path().join("fixture.7z");
        std::fs::write(&path, bytes).unwrap();
        let mut charts = 0;
        crate::song_archive::scan_charts(&path, &Default::default(), |entry, bytes| {
            assert_eq!(entry.name, "song/chart.bms");
            assert_eq!(
                String::from_utf8_lossy(bytes).replace("\r\n", "\n").as_bytes(),
                crate::song_archive::test_support::CHART
            );
            charts += 1;
            Ok(())
        })
        .unwrap();
        assert_eq!(charts, 1);
        let destination = root.path().join("extracted");
        crate::song_archive::extract_to(&path, &destination, &Default::default()).unwrap();
        assert_eq!(
            std::fs::read_to_string(destination.join("song/kick.wav")).unwrap().trim(),
            "WAVE fixture"
        );
    }
}

#[test]
fn altered_real_headers_reject_large_dictionaries_and_output_before_decoding() {
    let (dictionary, size) = encoded_fields();
    for (field, value, message) in [
        (dictionary, u32::MAX.to_le_bytes().to_vec(), "dictionary"),
        (size, number(MAX_DECODED_HEADER_BYTES + 1), "decoded header bytes"),
    ] {
        let mut changed = header(ENCODED).to_vec();
        changed.splice(field, value);
        let bytes = replace_header(ENCODED, &changed);
        let error = preflight_without_packed_reads(bytes.clone(), &Default::default()).unwrap_err();
        assert!(error.to_string().contains(message), "{error:#}");
        let root = tempfile::tempdir().unwrap();
        let path = root.path().join("attack.7z");
        std::fs::write(&path, bytes).unwrap();
        let error = crate::song_archive::inspect(&path, &Default::default()).unwrap_err();
        assert!(format!("{error:#}").contains(message), "{error:#}");
    }
}

fn simple_header(method: &[u8], properties: &[u8], size: u64, coders: usize) -> Vec<u8> {
    let mut bytes = vec![0x17, 6, 0, 1, 9, 1, 0, 7, 11, 1, 0, coders as u8];
    for _ in 0..coders {
        bytes.push(0x20 | method.len() as u8);
        bytes.extend(method);
        bytes.push(properties.len() as u8);
        bytes.extend(properties);
    }
    for i in 1..coders {
        bytes.extend([i as u8, (i - 1) as u8]);
    }
    bytes.push(12);
    for _ in 0..coders {
        bytes.extend(number(size));
    }
    bytes.extend([0, 0]);
    bytes
}

#[test]
fn header_resource_limits_accept_the_boundary_and_reject_one_more() {
    let limits = ArchiveLimits { dictionary_bytes: 4096, ..Default::default() };
    let bytes = simple_header(&[0x21], &[0], MAX_DECODED_HEADER_BYTES, 1);
    encoded(&bytes, 1000, &limits).unwrap();
    assert!(
        encoded(&bytes, 1000, &ArchiveLimits { dictionary_bytes: 4095, ..limits.clone() }).is_err()
    );
    assert!(
        encoded(
            &bytes,
            1000,
            &ArchiveLimits { header_bytes: MAX_DECODED_HEADER_BYTES - 1, ..limits.clone() }
        )
        .is_err()
    );
    assert!(
        encoded(&simple_header(&[0x21], &[0], 100, 2), 1000, &limits)
            .unwrap_err()
            .to_string()
            .contains("dictionary")
    );
    assert!(encoded(&simple_header(&[0x21], &[40], 100, 1), 1000, &limits).is_err());
    encoded(&simple_header(&[0], &[], 100, MAX_HEADER_CODERS), 1000, &limits).unwrap();
    assert!(encoded(&simple_header(&[0], &[], 100, MAX_HEADER_CODERS + 1), 1000, &limits).is_err());
    for method in [vec![0], vec![3, 1, 1], vec![0x0a], vec![0x0b], vec![3, 3, 1, 3]] {
        let properties = if method == [3, 1, 1] { vec![0x5d, 0, 0x10, 0, 0] } else { vec![] };
        encoded(&simple_header(&method, &properties, 100, 1), 1000, &limits).unwrap();
    }
    for method in [vec![3, 4, 1], vec![4, 2, 2], vec![6, 0xf1, 7, 1], vec![0xff]] {
        assert!(encoded(&simple_header(&method, &[], 100, 1), 1000, &limits).is_err());
    }
}

#[test]
fn malformed_numbers_truncated_headers_and_binding_cycles_never_panic() {
    assert_eq!(Input(&number(u64::MAX)).number().unwrap(), u64::MAX);
    for bytes in [&[0xff][..], &[0xff, 1], &[0xc0, 1], &[0x80]] {
        assert!(Input(bytes).number().is_err());
    }
    let bytes = header(ENCODED);
    for end in 0..bytes.len() {
        assert!(encoded(&bytes[..end], ENCODED.len() as u64, &Default::default()).is_err());
    }
    for index in 0..bytes.len() {
        for value in [0, 0xff, 0x80] {
            let mut changed = bytes.to_vec();
            changed[index] = value;
            let _ = encoded(&changed, ENCODED.len() as u64, &Default::default());
        }
    }
    let mut huge_count = vec![0x17, 6, 0];
    huge_count.extend(number(u64::MAX));
    assert!(encoded(&huge_count, 1000, &Default::default()).is_err());
    let mut cycle = simple_header(&[0], &[], 10, 2);
    cycle[18..20].copy_from_slice(&[0, 0]);
    assert!(encoded(&cycle, 1000, &Default::default()).is_err());
    for field in [12..20, 20..28] {
        let mut archive = ENCODED.to_vec();
        archive[field].copy_from_slice(&u64::MAX.to_le_bytes());
        let crc = checksum(&archive[12..32]);
        archive[8..12].copy_from_slice(&crc.to_le_bytes());
        assert!(preflight(&mut Cursor::new(archive), &Default::default()).is_err());
    }
    let mut recovery_header = ENCODED.to_vec();
    recovery_header[8..32].fill(0);
    assert!(preflight(&mut Cursor::new(recovery_header), &Default::default()).is_err());
}
