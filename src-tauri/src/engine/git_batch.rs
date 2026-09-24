/// Splits `git cat-file --batch` output (`<oid> blob <size>\n<size bytes>\n` per object) into
/// `count` contents. None when an object is missing, is not a blob, or the output does not add up.
pub fn parse(mut out: &[u8], count: usize) -> Option<Vec<Vec<u8>>> {
    let mut blobs = Vec::with_capacity(count);
    for _ in 0..count {
        let end = memchr::memchr(b'\n', out)?;
        let mut header = std::str::from_utf8(&out[..end]).ok()?.split(' ');
        let (_oid, kind, size) = (header.next()?, header.next()?, header.next()?.parse::<usize>().ok()?);
        let body = out.get(end + 1..end + 1 + size)?;
        if kind != "blob" || out.get(end + 1 + size) != Some(&b'\n') {
            return None;
        }
        blobs.push(body.to_vec());
        out = &out[end + 2 + size..];
    }
    Some(blobs)
}

#[cfg(test)]
mod tests {
    use super::parse;

    #[test]
    fn splits_binary_blobs_and_refuses_anything_else() {
        let out = b"aaaa blob 3\n\x00\n\xff\nbbbb blob 0\n\n";
        assert_eq!(parse(out, 2), Some(vec![b"\x00\n\xff".to_vec(), Vec::new()]));
        assert_eq!(parse(b"HEAD:p/x missing\n", 1), None);
        assert_eq!(parse(b"aaaa blob 10\nshort\n", 1), None);
        assert_eq!(parse(b"aaaa tree 1\nx\n", 1), None);
        assert_eq!(parse(b"aaaa blob 1\nxy", 1), None);
    }
}
