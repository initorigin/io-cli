//! The clipboard write: a well-formed OSC 52 sequence, and wording that does not
//! claim an acknowledgement the terminal never sends.
//!
//! The base64 vectors here are the RFC 4648 ones, checked by hand rather than
//! produced by the encoder under test — an expectation generated from the code it
//! is testing agrees with every bug in it.

use io_cli::clipboard::{describe, encode, sequence};

const ALPHABET: &str = "ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";

/// The other direction, written out here so the round trip is checked against
/// something independent of `encode`.
fn decode(text: &str) -> Vec<u8> {
    let mut bytes = Vec::new();
    let mut bits: u32 = 0;
    let mut held = 0u32;
    for character in text.chars().filter(|character| *character != '=') {
        let value = ALPHABET
            .find(character)
            .unwrap_or_else(|| panic!("{character:?} is not a base64 character"));
        bits = (bits << 6) | value as u32;
        held += 6;
        if held >= 8 {
            held -= 8;
            bytes.push((bits >> held) as u8);
        }
    }
    bytes
}

/// Split a sequence into its three parts by position, never by searching for the
/// payload inside it: a `contains` check passes just as happily on a sequence
/// whose prefix and payload came out the wrong way round.
fn payload_of(sequence: &str) -> &str {
    let body = sequence
        .strip_prefix("\x1b]52;c;")
        .expect("the sequence opens with OSC 52, selection c");
    body.strip_suffix('\x07')
        .expect("the sequence is terminated by BEL")
}

#[test]
fn the_sequence_is_osc_52_around_the_encoded_payload() {
    let text = "the quick brown fox";
    let written = sequence(text);

    let encoded = payload_of(&written);
    assert_eq!(
        encoded, "dGhlIHF1aWNrIGJyb3duIGZveA==",
        "the middle of the sequence is not the base64 of the payload",
    );
    assert_eq!(
        decode(encoded),
        text.as_bytes(),
        "the payload did not survive the round trip",
    );

    // Positions, not membership. The introducer belongs at the front and the
    // terminator at the back, and only one of each may exist.
    assert_eq!(written.find('\x1b'), Some(0), "got {written:?}");
    assert_eq!(
        written.find('\x07'),
        Some(written.len() - 1),
        "a BEL before the end would terminate the sequence early: {written:?}",
    );
    assert_eq!(written.matches('\x07').count(), 1, "got {written:?}");
}

#[test]
fn a_payload_carrying_a_terminator_cannot_end_the_sequence_early() {
    // Tool output is arbitrary bytes, so a transcript really can contain a BEL or
    // an ESC. Encoded, neither reaches the terminal as itself.
    let text = "before\x07after\x1b]0;title\x07";
    let written = sequence(text);
    let encoded = payload_of(&written);

    assert!(
        !encoded.contains('\x07') && !encoded.contains('\x1b'),
        "a control byte reached the wire unencoded: {encoded:?}",
    );
    assert_eq!(
        decode(encoded),
        text.as_bytes(),
        "the round trip lost bytes"
    );
}

#[test]
fn base64_pads_correctly_at_every_length() {
    // RFC 4648's own vectors, at the three residues modulo three and either side
    // of the first full group.
    let vectors: [(&str, &str); 5] = [
        ("", ""),
        ("f", "Zg=="),
        ("fo", "Zm8="),
        ("foo", "Zm9v"),
        ("foob", "Zm9vYg=="),
    ];
    for (input, expected) in vectors {
        assert_eq!(
            encode(input.as_bytes()),
            expected,
            "{input:?} encoded wrong",
        );
    }
}

#[test]
fn the_last_two_characters_of_the_alphabet_are_plus_and_slash() {
    // The URL-safe variant differs from the standard one in exactly these two
    // characters, and it differs silently: a `-_` payload decodes to other bytes
    // rather than being rejected, so a terminal would paste something else.
    assert_eq!(encode(&[0xFB, 0xFF, 0xFE]), "+//+");
    assert_eq!(encode(&[0xFF, 0xFF, 0xFF]), "////");
}

#[test]
fn multi_byte_utf8_survives_the_round_trip() {
    let text = "café ☕ — 日本語 🙂";
    let written = sequence(text);
    let decoded = decode(payload_of(&written));

    assert_eq!(decoded, text.as_bytes(), "the bytes changed");
    assert_eq!(
        String::from_utf8(decoded).expect("still valid UTF-8"),
        text,
        "the text changed",
    );
}

#[test]
fn the_description_states_a_size_and_claims_nothing_about_the_clipboard() {
    // Nothing on the wire answers an OSC 52 write, and tmux drops it outright
    // without `set -g set-clipboard on`. Any of these words would be io-cli
    // asserting an outcome it never observed.
    let large = "a".repeat(4200);
    for payload in ["", "x", "the quick brown fox", large.as_str()] {
        let description = describe(payload).to_lowercase();
        for claim in ["copied", "success", "done"] {
            assert!(
                !description.contains(claim),
                "{description:?} claims {claim:?}, which the terminal never confirmed",
            );
        }
        assert!(
            description.starts_with("sent "),
            "the description should open with what io-cli did: {description:?}",
        );
    }

    assert_eq!(describe(""), "sent 0 bytes to the terminal clipboard");
    assert_eq!(describe("x"), "sent 1 byte to the terminal clipboard");
    assert_eq!(describe(&large), "sent 4.2 kB to the terminal clipboard");
    // Bytes, not characters: a terminal's undocumented ceiling is a byte count,
    // and this string is four characters long.
    assert_eq!(
        describe("日本語 "),
        "sent 10 bytes to the terminal clipboard"
    );
}
