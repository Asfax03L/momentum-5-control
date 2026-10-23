#![no_main]

use libfuzzer_sys::fuzz_target;
use momentum_core::protocol::{Frame, FrameDecoder, HEADER_LEN, MAX_PAYLOAD_LEN};

fn drain(decoder: &mut FrameDecoder, frames: &mut Vec<Frame>) {
    loop {
        let before = decoder.pending().len();
        let next = decoder.next_frame();
        if let Some(next) = next {
            // Une extraction ou une erreur doit avancer : sinon la boucle
            // d'un lecteur réel ne pourrait jamais se terminer.
            assert!(decoder.pending().len() < before);
            if let Ok(frame) = next {
                frames.push(frame);
            }
        } else {
            assert_eq!(decoder.pending().len(), before);
            assert!(before < HEADER_LEN + MAX_PAYLOAD_LEN);
            break;
        }
    }
}

fuzz_target!(|data: &[u8]| {
    // Préfixe : nombre (modulo 17) de tailles, puis ces tailles moins un.
    // Le reste est le flux brut, découpé selon ce plan cyclique. Sans plan,
    // on avance octet par octet. Les tailles produites vont de 1 à 256.
    let Some((&count, rest)) = data.split_first() else {
        return;
    };
    let count = usize::from(count % 17);
    let Some(plan) = rest.get(..count) else {
        return;
    };
    let stream = &rest[count..];

    let mut whole = FrameDecoder::new();
    whole.push(stream);
    let mut expected = Vec::new();
    drain(&mut whole, &mut expected);

    let mut split = FrameDecoder::new();
    let mut actual = Vec::new();
    let (mut offset, mut step) = (0, 0);
    while offset < stream.len() {
        let size = if plan.is_empty() {
            1
        } else {
            usize::from(plan[step % plan.len()]) + 1
        };
        let end = (offset + size).min(stream.len());
        split.push(&stream[offset..end]);
        drain(&mut split, &mut actual);
        offset = end;
        step += 1;
    }
    drain(&mut split, &mut actual);
    assert_eq!(actual, expected);
    assert_eq!(split.pending(), whole.pending());
});
