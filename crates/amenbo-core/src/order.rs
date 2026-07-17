//! Ordering by fractional index: keys that fall between other keys.
//!
//! Positions are string keys rather than array indices, and a key can always be generated *between* two
//! others, so **moving one element is complete in itself** — no other element has to be updated.
//!
//! Keys are base-26 over `a`..`z`, chosen so that lexicographic (byte) order on the string is the sort
//! order.

const ALPHABET: &[u8] = b"abcdefghijklmnopqrstuvwxyz";
const N: i32 = 26;

/// The digit value at position `i`. Past the end of the string this is `-1`, standing for "before that
/// digit exists at all", i.e. below the minimum.
fn val_at(s: &str, i: usize) -> i32 {
    match s.as_bytes().get(i) {
        Some(&b) => (b - b'a') as i32,
        None => -1,
    }
}

/// Return a new key strictly between `lo` and `hi` (`lo < result < hi`).
///
/// - `lo = None` — before the first element
/// - `hi = None` — after the last element
/// - both `None` — the first key in an empty set
pub fn key_between(lo: Option<&str>, hi: Option<&str>) -> String {
    let lo = lo.unwrap_or("");
    let mut out = String::new();
    let mut i = 0usize;
    loop {
        let av = val_at(lo, i);
        let bv = match hi {
            // Once hi runs out of digits, treat it as one past the maximum (N), which keeps the
            // result on the smaller-than-hi side.
            Some(h) => match h.as_bytes().get(i) {
                Some(&b) => (b - b'a') as i32,
                None => N,
            },
            None => N,
        };

        if av == bv {
            // The digits agree: fix this one and move on to the next.
            out.push(ALPHABET[av as usize] as char);
            i += 1;
            continue;
        }

        let mid = (av + bv) / 2;
        if mid > av {
            out.push(ALPHABET[mid as usize] as char);
            return out;
        }

        // The digits are adjacent (mid == av): follow lo down and open the gap at the next digit.
        out.push(ALPHABET[av as usize] as char);
        i += 1;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn assert_between(lo: Option<&str>, hi: Option<&str>) -> String {
        let k = key_between(lo, hi);
        if let Some(l) = lo {
            assert!(k.as_str() > l, "{k:?} must be greater than {l:?}");
        }
        if let Some(h) = hi {
            assert!(k.as_str() < h, "{k:?} must be less than {h:?}");
        }
        k
    }

    #[test]
    fn first_key_is_mid() {
        let k = assert_between(None, None);
        assert_eq!(k, "m");
    }

    #[test]
    fn append_and_prepend() {
        let a = assert_between(None, None);
        let after = assert_between(Some(&a), None);
        let before = assert_between(None, Some(&a));
        assert!(before < a && a < after);
    }

    #[test]
    fn insert_between_adjacent() {
        let a = "a".to_string();
        let b = "b".to_string();
        let mid = assert_between(Some(&a), Some(&b));
        assert!(a < mid && mid < b);
    }

    #[test]
    fn repeated_inserts_between_same_pair_stay_ordered() {
        let mut lo = "a".to_string();
        let hi = "b".to_string();
        let mut prev = lo.clone();
        for _ in 0..50 {
            let k = key_between(Some(&lo), Some(&hi));
            assert!(k > prev, "{k:?} > {prev:?}");
            assert!(k < hi, "{k:?} < {hi:?}");
            prev = k.clone();
            lo = k;
        }
    }

    #[test]
    fn sequential_append_keeps_order() {
        let mut keys: Vec<String> = Vec::new();
        let mut last: Option<String> = None;
        for _ in 0..100 {
            let k = key_between(last.as_deref(), None);
            if let Some(l) = &last {
                assert!(&k > l);
            }
            last = Some(k.clone());
            keys.push(k);
        }
        let mut sorted = keys.clone();
        sorted.sort();
        assert_eq!(keys, sorted);
    }
}
